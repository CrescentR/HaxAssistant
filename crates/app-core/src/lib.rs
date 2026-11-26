pub mod ocr_worker;
pub mod player_store;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use data_runtime::Dataset;
use data_updater::{inspect_asset_status, verify_manifest, ManifestReport};
use lcu_client::sgp::{ocr_lang_from_locale, parse_sgp_match_history};
use lcu_client::{discover_lockfile, parse_match_history, LcuClient};
use ocr_worker::{cleanup_ocr_cache as cleanup_ocr_cache_files, OcrLauncher, PersistentOcrWorker};
use player_store::PlayerStore;
use serde_json::{json, Value};
use shared_types::{
    AppSettings, ArurfRecommendation, AssetSyncResult, AssetSyncStatus, AugmentOfferStatsReport,
    AugmentRecommendationCard, BenchGrabStatus, EnemyCounterAnalysis, GamePlayerIntel,
    IntelCacheStatus, ItemPlan, ItemSetSyncResult, LcuStatus, LeagueWindowMode, LineupEvaluation,
    LiveGameSnapshot,
    MatchHistoryEntry, MatchHistoryResponse, OcrCacheCleanupReport, OverlayStatePayload,
    PlayerEncounterRecord, PlayerEncounterSummary, PlayerLibraryBundle, PlayerMarker, PlayerProfile,
    PlayerSearchResult, PostgameReview, RunePagePlan, RuntimeSummary, SummonerSpellSnapshot,
};

const INTEL_CACHE_TTL_SECS: u64 = 300;

#[derive(Debug, Clone)]
struct CachedIntelEntry {
    intel: GamePlayerIntel,
    fetched_at_epoch: u64,
}

#[derive(Debug, Clone)]
pub struct LcuPollOutcome {
    pub status: LcuStatus,
    pub overlay_state: OverlayStatePayload,
    pub changed: bool,
    pub auto_accepted: bool,
    pub auto_grabbed_champion_id: Option<u32>,
}

pub struct HexBoxCore {
    pub root: PathBuf,
    dataset: RwLock<Arc<Dataset>>,
    model_dir: PathBuf,
    pub lcu: LcuClient,
    pub player_store: PlayerStore,
    settings: RwLock<AppSettings>,
    overlay_state: RwLock<OverlayStatePayload>,
    ocr_worker: Mutex<PersistentOcrWorker>,
    ready_check_seen_at: Mutex<Option<Instant>>,
    pending_bench_grab: Mutex<Option<u32>>,
    bench_autograb_cancelled: Mutex<bool>,
    last_auto_spell_champ_id: Mutex<Option<u32>>,
    /// Whether the current champ-select lobby is an ARAM(-family) queue; None
    /// until the gameflow session has been checked this phase.
    champ_select_is_aram: Mutex<Option<bool>>,
    last_recorded_live_key: Mutex<Option<String>>,
    intel_cache: RwLock<HashMap<String, CachedIntelEntry>>,
    last_intel_refresh_epoch: RwLock<Option<u64>>,
}

/// Normalize a summoner/riot-id name for equality checks: strip any `#tag`,
/// trim, and case-fold. LCU `current-summoner` and liveclientdata report names
/// in different formats (`GameName` vs `GameName#Tag`), so raw comparison
/// fails to recognize the same player.
fn normalize_summoner_name(name: &str) -> String {
    name.split('#')
        .next()
        .unwrap_or(name)
        .trim()
        .to_lowercase()
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl HexBoxCore {
    pub fn initialize(root: &Path) -> Result<Self> {
        let dataset = Dataset::load_from_root(root)?;
        let data_dir = dataset.data_dir.clone();
        let settings = load_settings_file(&data_dir);
        let launcher = resolve_ocr_launcher(root);
        let model_dir = resolve_model_dir(root);
        let ocr_worker =
            PersistentOcrWorker::new(launcher, data_dir.clone(), model_dir.clone());
        let player_store = PlayerStore::load(&data_dir);

        Ok(Self {
            root: root.to_path_buf(),
            dataset: RwLock::new(Arc::new(dataset)),
            model_dir,
            lcu: LcuClient::new(),
            player_store,
            settings: RwLock::new(settings),
            overlay_state: RwLock::new(OverlayStatePayload::default()),
            ocr_worker: Mutex::new(ocr_worker),
            ready_check_seen_at: Mutex::new(None),
            pending_bench_grab: Mutex::new(None),
            bench_autograb_cancelled: Mutex::new(false),
            last_auto_spell_champ_id: Mutex::new(None),
            champ_select_is_aram: Mutex::new(None),
            last_recorded_live_key: Mutex::new(None),
            intel_cache: RwLock::new(HashMap::new()),
            last_intel_refresh_epoch: RwLock::new(None),
        })
    }

    pub fn dataset(&self) -> Arc<Dataset> {
        self.dataset.read().unwrap().clone()
    }

    pub fn data_dir(&self) -> PathBuf {
        self.dataset().data_dir.clone()
    }

    pub fn model_dir(&self) -> PathBuf {
        self.model_dir.clone()
    }

    pub fn reload_dataset(&self) -> Result<RuntimeSummary> {
        let fresh = Dataset::load_from_root(&self.root)?;
        if let Ok(mut guard) = self.dataset.write() {
            *guard = Arc::new(fresh);
        }
        Ok(self.runtime_summary())
    }

    pub fn runtime_summary(&self) -> RuntimeSummary {
        let ds = self.dataset();
        RuntimeSummary {
            patch: ds.patch().to_string(),
            report_date: ds.report_date().to_string(),
            hero_count: ds.champions.len(),
            augment_count: ds.augments_full.len(),
            mechanic_link_count: ds.mechanic_pairs.links.len(),
            mechanic_conflict_count: ds.mechanic_pairs.conflicts.len(),
            data_root: ds.data_dir.display().to_string(),
        }
    }

    pub fn get_settings(&self) -> AppSettings {
        self.settings
            .read()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    pub fn update_settings(&self, next: AppSettings) -> Result<AppSettings> {
        if let Ok(mut guard) = self.settings.write() {
            *guard = next.clone();
        }
        save_settings_file(&self.data_dir(), &next)?;
        Ok(next)
    }

    pub fn get_overlay_state(&self) -> OverlayStatePayload {
        self.overlay_state
            .read()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    pub fn set_overlay_state(&self, next: OverlayStatePayload) -> OverlayStatePayload {
        self.set_overlay_state_if_changed(next).0
    }

    pub fn set_overlay_state_if_changed(
        &self,
        next: OverlayStatePayload,
    ) -> (OverlayStatePayload, bool) {
        if let Ok(mut guard) = self.overlay_state.write() {
            if *guard == next {
                return (guard.clone(), false);
            }
            *guard = next.clone();
            return (next, true);
        }
        (next, false)
    }

    pub fn champion_name_by_id(&self, id: u32) -> Option<String> {
        let id_str = id.to_string();
        let ds = self.dataset();
        ds.champions
            .iter()
            .find(|c| c.id == id_str)
            .map(|c| c.hero.clone())
    }

    pub fn champion_id_by_name(&self, name: &str) -> Option<u32> {
        let ds = self.dataset();
        ds.champions
            .iter()
            .find(|c| c.hero == name || c.id == name)
            .and_then(|c| c.id.parse::<u32>().ok())
    }

    pub fn augment_name_by_id(&self, id: u32) -> Option<String> {
        let ds = self.dataset();
        ds.augments_full
            .iter()
            .find(|a| a.augment_id == id)
            .map(|a| a.augment_name.clone())
    }

    // -----------------------------------------------------------------------
    // P0: Data/Model sync, OCR cache cleanup, client locale
    // -----------------------------------------------------------------------

    pub fn get_asset_sync_status(&self) -> AssetSyncStatus {
        inspect_asset_status(&self.data_dir(), &self.model_dir)
    }

    pub async fn sync_remote_assets(
        &self,
        source_url: Option<&str>,
        include_models: bool,
    ) -> Result<AssetSyncResult> {
        let url = source_url
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .or_else(|| {
                let s = self.get_settings().data_sync_url;
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .or_else(|| {
                std::env::var("HAXASSISTANT_DATA_SYNC_URL")
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                option_env!("HAXASSISTANT_DATA_SYNC_URL")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToString::to_string)
            })
            .unwrap_or_else(|| {
                if cfg!(debug_assertions) {
                    "http://127.0.0.1:8787".to_string()
                } else {
                    String::new()
                }
            });
        let result =
            data_updater::sync_remote_assets(&url, &self.data_dir(), &self.model_dir, include_models)
                .await?;
        let _ = self.reload_dataset();
        Ok(result)
    }

    pub fn cleanup_ocr_cache(&self) -> OcrCacheCleanupReport {
        cleanup_ocr_cache_files(&self.data_dir())
    }

    pub async fn get_client_locale(&self) -> String {
        if let Some(creds) = discover_lockfile() {
            if let Some(locale) = self.lcu.fetch_client_locale(&creds).await {
                return locale;
            }
        }
        self.get_overlay_state()
            .lcu
            .locale
            .unwrap_or_else(|| "zh_CN".to_string())
    }

    fn current_ocr_lang(&self) -> &'static str {
        let locale = self
            .get_overlay_state()
            .lcu
            .locale
            .unwrap_or_else(|| "zh_CN".to_string());
        ocr_lang_from_locale(&locale)
    }

    pub fn verify_data_manifest(&self) -> Result<ManifestReport> {
        verify_manifest(&self.data_dir())
    }

    // -----------------------------------------------------------------------
    // Recommendations, Manual Augment Overrides & Offer Stats
    // -----------------------------------------------------------------------

    pub fn evaluate_augment_offer(
        &self,
        hero_name: &str,
        candidate_names: &[String],
        stage: u8,
        confirmed_augments: &[String],
    ) -> Vec<AugmentRecommendationCard> {
        let mode = self.get_settings().augment_value_mode;
        let ds = self.dataset();
        reco_engine::evaluate_augment_offer_with_mode(
            &ds,
            hero_name,
            candidate_names,
            stage,
            confirmed_augments,
            &mode,
        )
    }

    pub fn set_manual_augment_selection(
        &self,
        confirmed_augments: Vec<String>,
    ) -> OverlayStatePayload {
        let prev = self.get_overlay_state().confirmed_augments;
        for aug in &confirmed_augments {
            if !prev.contains(aug) {
                self.player_store.record_augment_pick(aug);
            }
        }
        let mut state = self.get_overlay_state();
        state.confirmed_augments = confirmed_augments;
        state.stage = (state.confirmed_augments.len() as u8 + 1).clamp(1, 4);
        self.set_overlay_state(state)
    }

    /// Manually override a single candidate slot (1..=3) when OCR misreads or user rerolls.
    pub fn set_manual_augment_override(
        &self,
        slot: usize,
        augment_name: String,
    ) -> Result<OverlayStatePayload> {
        if !(1..=3).contains(&slot) {
            return Err(anyhow!("slot must be 1, 2, or 3"));
        }
        let trimmed = augment_name.trim().to_string();
        if trimmed.is_empty() {
            return Err(anyhow!("augment name cannot be empty"));
        }
        let mut state = self.get_overlay_state();
        while state.candidate_augments.len() < 3 {
            state.candidate_augments.push(String::new());
        }
        state.candidate_augments[slot - 1] = trimmed.clone();
        state.ocr_status = Some(format!("已手动修正槽位 #{slot} → {trimmed}"));
        Ok(self.set_overlay_state(state))
    }

    pub fn get_augment_offer_stats(&self, top_n: usize) -> AugmentOfferStatsReport {
        self.player_store.get_offer_stats(top_n)
    }

    pub fn analyze_enemy_counter(
        &self,
        ally_hero_name: &str,
        enemy_hero_names: &[String],
    ) -> EnemyCounterAnalysis {
        reco_engine::analyze_enemy_counter(&self.dataset(), ally_hero_name, enemy_hero_names)
    }

    pub fn evaluate_5v5_lineup(
        &self,
        ally_names: &[String],
        enemy_names: &[String],
        bench_names: &[String],
        local_hero_name: &str,
    ) -> LineupEvaluation {
        reco_engine::evaluate_5v5_lineup(
            &self.dataset(),
            ally_names,
            enemy_names,
            bench_names,
            local_hero_name,
        )
    }

    /// Purchase order for the hero currently in the overlay state (live game),
    /// or for the explicitly given hero / gold / inventory.
    pub fn item_plan(
        &self,
        hero_name: Option<&str>,
        current_gold: Option<f64>,
        owned_item_ids: Option<&[u32]>,
        enemy_names: Option<&[String]>,
    ) -> ItemPlan {
        let state = self.get_overlay_state();
        reco_engine::plan_item_order(
            &self.dataset(),
            hero_name.unwrap_or(&state.hero_name),
            current_gold.unwrap_or(state.current_gold),
            owned_item_ids.unwrap_or(&state.owned_item_ids),
            enemy_names.unwrap_or(&state.enemy_team),
        )
    }

    pub fn postgame_review(&self, entry: &MatchHistoryEntry) -> PostgameReview {
        reco_engine::review_match(&self.dataset(), entry)
    }

    // -----------------------------------------------------------------------
    // P1: Summoner Spells, Item Set Sync, ARURF & Runes
    // -----------------------------------------------------------------------

    pub async fn get_summoner_spell_snapshot(&self) -> SummonerSpellSnapshot {
        let settings = self.get_settings();
        let state = self.get_overlay_state();
        let ds = self.dataset();

        let mut in_champ_select = false;
        let mut champion_id = self.champion_id_by_name(&state.hero_name);
        let mut hero_name = Some(state.hero_name.clone());
        let mut spell1_id = None;
        let mut spell2_id = None;

        if let Some(creds) = discover_lockfile() {
            if let Ok((cid, s1, s2)) = self.lcu.fetch_current_summoner_spells(&creds).await {
                in_champ_select = true;
                if let Some(id) = cid {
                    champion_id = Some(id);
                    hero_name = self.champion_name_by_id(id).or(hero_name);
                }
                spell1_id = s1;
                spell2_id = s2;
            }
        }

        let target_hero = hero_name.as_deref().unwrap_or(&state.hero_name);
        let (rec1, rec2) =
            reco_engine::recommend_aram_spells(&ds, target_hero, &settings.summoner_flash_slot);

        SummonerSpellSnapshot {
            in_champ_select,
            champion_id,
            hero_name,
            spell1_id,
            spell2_id,
            spell1_name: spell1_id
                .map(|id| reco_engine::summoner_spell_name(id).to_string())
                .unwrap_or_else(|| "未读取".to_string()),
            spell2_name: spell2_id
                .map(|id| reco_engine::summoner_spell_name(id).to_string())
                .unwrap_or_else(|| "未读取".to_string()),
            recommended_spell1_id: rec1,
            recommended_spell2_id: rec2,
            recommended_spell1_name: reco_engine::summoner_spell_name(rec1).to_string(),
            recommended_spell2_name: reco_engine::summoner_spell_name(rec2).to_string(),
            flash_slot: settings.summoner_flash_slot,
        }
    }

    pub async fn apply_summoner_spells(
        &self,
        spell1_id: Option<u32>,
        spell2_id: Option<u32>,
    ) -> Result<SummonerSpellSnapshot> {
        let creds = discover_lockfile().ok_or_else(|| anyhow!("未检测到运行中的英雄联盟客户端"))?;
        let snap = self.get_summoner_spell_snapshot().await;
        let s1 = spell1_id.unwrap_or(snap.recommended_spell1_id);
        let s2 = spell2_id.unwrap_or(snap.recommended_spell2_id);
        self.lcu.apply_summoner_spells(&creds, s1, s2).await?;
        Ok(self.get_summoner_spell_snapshot().await)
    }

    // -----------------------------------------------------------------------
    // Game window mode (borderless fix)
    // -----------------------------------------------------------------------

    /// League's stored window mode. Overlays can only be drawn over the game
    /// in windowed or borderless mode, not exclusive fullscreen.
    pub async fn get_game_window_mode(&self) -> LeagueWindowMode {
        let Some(creds) = discover_lockfile() else {
            return lcu_client::window_mode_status(false, None);
        };
        let mode = self
            .lcu
            .fetch_game_window_mode(&creds)
            .await
            .ok()
            .flatten();
        lcu_client::window_mode_status(true, mode)
    }

    /// Switch the game to borderless. Applies from the next game launch.
    pub async fn fix_game_borderless(&self) -> Result<LeagueWindowMode> {
        let creds = discover_lockfile().ok_or_else(|| anyhow!("未检测到运行中的英雄联盟客户端"))?;
        self.lcu
            .set_game_window_mode(&creds, lcu_client::WINDOW_MODE_BORDERLESS)
            .await?;
        Ok(self.get_game_window_mode().await)
    }

    pub async fn sync_item_set_to_client(
        &self,
        hero_name: Option<&str>,
    ) -> Result<ItemSetSyncResult> {
        let state = self.get_overlay_state();
        let target_hero = hero_name.unwrap_or(&state.hero_name);
        let ds = self.dataset();
        let champion_id = self
            .champion_id_by_name(target_hero)
            .ok_or_else(|| anyhow!("未找到英雄 {target_hero} 的 ID"))?;

        let build = ds
            .formula_items_by_hero
            .get(target_hero)
            .or_else(|| ds.formula_items_by_hero.get(&champion_id.to_string()))
            .cloned()
            .ok_or_else(|| anyhow!("暂无英雄 {target_hero} 的公式出装数据"))?;

        let counter = reco_engine::analyze_enemy_counter(&ds, target_hero, &state.enemy_team);

        let mut blocks = Vec::new();
        let mut total_items = 0usize;

        if !build.starter_items.is_empty() {
            let items: Vec<Value> = build
                .starter_items
                .iter()
                .map(|it| json!({ "id": it.id, "count": 1 }))
                .collect();
            total_items += items.len();
            blocks.push(json!({ "type": "出门装 · HaxAssistant", "items": items }));
        }
        if !build.core_items.is_empty() {
            let items: Vec<Value> = build
                .core_items
                .iter()
                .map(|it| json!({ "id": it.id, "count": 1 }))
                .collect();
            total_items += items.len();
            blocks.push(json!({ "type": format!("核心公式装 · {}", build.label), "items": items }));
        }
        let mut situational = Vec::new();
        if let Some(boots) = counter.boots {
            situational.push(json!({ "id": boots.item_id.to_string(), "count": 1 }));
        }
        if let Some(ah) = counter.anti_heal {
            situational.push(json!({ "id": ah.item_id.to_string(), "count": 1 }));
        }
        for def in counter.defensive_items.into_iter().take(2) {
            situational.push(json!({ "id": def.item_id.to_string(), "count": 1 }));
        }
        if !situational.is_empty() {
            total_items += situational.len();
            blocks.push(json!({ "type": "针对/防装推荐", "items": situational }));
        }

        let title = format!("HaxAssistant · {target_hero} 大乱斗出装");
        let Some(creds) = discover_lockfile() else {
            return Ok(ItemSetSyncResult {
                hero_name: target_hero.to_string(),
                champion_id,
                title,
                item_count: total_items,
                synced_to_client: false,
                message: "未检测到英雄联盟客户端，已生成出装方案但未写入客户端".to_string(),
            });
        };

        self.lcu
            .sync_item_set(&creds, champion_id, &title, blocks)
            .await?;
        Ok(ItemSetSyncResult {
            hero_name: target_hero.to_string(),
            champion_id,
            title,
            item_count: total_items,
            synced_to_client: true,
            message: format!("已同步【{target_hero}】装备方案到英雄联盟客户端（共 {total_items} 件装备）"),
        })
    }

    pub fn get_arurf_recommendation(&self, hero_name: Option<&str>) -> ArurfRecommendation {
        let state = self.get_overlay_state();
        let settings = self.get_settings();
        let target = hero_name.unwrap_or(&state.hero_name);
        reco_engine::build_arurf_recommendation(&self.dataset(), target, &settings.summoner_flash_slot)
    }

    pub async fn apply_arurf_rune_page(&self, hero_name: Option<&str>) -> Result<RunePagePlan> {
        let creds = discover_lockfile().ok_or_else(|| anyhow!("未检测到运行中的英雄联盟客户端"))?;
        let rec = self.get_arurf_recommendation(hero_name);
        self.lcu.apply_rune_page(&creds, &rec.rune_page).await?;
        Ok(rec.rune_page)
    }

    // -----------------------------------------------------------------------
    // P1: Player Markers, Encounters & Library
    // -----------------------------------------------------------------------

    pub fn list_player_markers(&self) -> Vec<PlayerMarker> {
        self.player_store.list_markers()
    }

    pub fn save_player_marker(&self, marker: PlayerMarker) -> Result<PlayerMarker> {
        self.player_store.save_marker(marker)
    }

    pub fn delete_player_marker(&self, puuid_or_name: &str) -> Result<bool> {
        self.player_store.delete_marker(puuid_or_name)
    }

    pub fn list_player_encounters(&self, limit: usize) -> Vec<PlayerEncounterRecord> {
        self.player_store.list_encounters(limit)
    }

    pub fn get_encounter_summary(&self, query: &str) -> PlayerEncounterSummary {
        self.player_store.encounter_summary(query)
    }

    pub fn delete_player_encounters(&self, puuid_or_name: &str) -> Result<usize> {
        self.player_store.delete_encounters(puuid_or_name)
    }

    pub fn prune_player_encounters(&self, max_age_days: u64) -> Result<usize> {
        self.player_store.prune_encounters(max_age_days)
    }

    pub fn export_player_library(&self) -> PlayerLibraryBundle {
        self.player_store.export_library()
    }

    pub fn import_player_library(
        &self,
        bundle: PlayerLibraryBundle,
        replace: bool,
    ) -> Result<(usize, usize)> {
        self.player_store.import_library(bundle, replace)
    }

    // -----------------------------------------------------------------------
    // P1: SGP / LCU Player Search, Profile & 10-Player Game Intel
    // -----------------------------------------------------------------------

    pub async fn fetch_personal_match_history(&self, count: usize) -> MatchHistoryResponse {
        let Some(creds) = discover_lockfile() else {
            return MatchHistoryResponse {
                source: "offline".to_string(),
                summoner_name: None,
                matches: Vec::new(),
            };
        };

        let status = self.lcu.probe_status().await;
        match self.lcu.fetch_match_history(&creds, count).await {
            Ok(raw) => {
                let matches = parse_match_history(
                    &raw,
                    |cid| {
                        self.champion_name_by_id(cid)
                            .unwrap_or_else(|| format!("英雄 #{cid}"))
                    },
                    |aid| self.augment_name_by_id(aid),
                );
                MatchHistoryResponse {
                    source: "lcu".to_string(),
                    summoner_name: status.summoner_name,
                    matches,
                }
            }
            Err(_) => MatchHistoryResponse {
                source: "offline".to_string(),
                summoner_name: status.summoner_name,
                matches: Vec::new(),
            },
        }
    }

    pub async fn search_player(&self, query: &str) -> Result<PlayerSearchResult> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("请输入召唤师名称（支持 名称#Tag）"));
        }

        if let Some(creds) = discover_lockfile() {
            if let Ok(raw) = self.lcu.lookup_summoner(&creds, trimmed).await {
                let puuid = raw
                    .get("puuid")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let summoner_id = raw.get("summonerId").and_then(|v| v.as_u64());
                let game_name = raw
                    .get("gameName")
                    .or_else(|| raw.get("displayName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(trimmed);
                let tag_line = raw
                    .get("tagLine")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let full_name = if tag_line.is_empty() {
                    game_name.to_string()
                } else {
                    format!("{game_name}#{tag_line}")
                };
                let marker = self.player_store.find_marker(&puuid, &full_name);
                let encounter_count = self
                    .player_store
                    .encounter_summary(&full_name)
                    .total_encounters;
                return Ok(PlayerSearchResult {
                    puuid,
                    summoner_id,
                    summoner_name: full_name,
                    tag_line,
                    summoner_level: raw
                        .get("summonerLevel")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32,
                    profile_icon_id: raw
                        .get("profileIconId")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32,
                    source: "lcu".to_string(),
                    marker,
                    encounter_count,
                });
            }
        }

        // Offline / fallback search against local markers & encounters.
        let marker = self.player_store.find_marker(trimmed, trimmed);
        let summary = self.player_store.encounter_summary(trimmed);
        if let Some(m) = marker {
            return Ok(PlayerSearchResult {
                puuid: m.puuid.clone(),
                summoner_id: None,
                summoner_name: m.summoner_name.clone(),
                tag_line: String::new(),
                summoner_level: 0,
                profile_icon_id: 0,
                source: "local".to_string(),
                marker: Some(m),
                encounter_count: summary.total_encounters,
            });
        }
        if summary.total_encounters > 0 {
            let first = &summary.recent[0];
            return Ok(PlayerSearchResult {
                puuid: first.puuid.clone(),
                summoner_id: None,
                summoner_name: first.summoner_name.clone(),
                tag_line: String::new(),
                summoner_level: 0,
                profile_icon_id: 0,
                source: "local".to_string(),
                marker: None,
                encounter_count: summary.total_encounters,
            });
        }

        Err(anyhow!("未找到玩家「{trimmed}」（请确认英雄联盟客户端已运行，或名称/Tag 拼写正确）"))
    }

    async fn fetch_matches_for_puuid(
        &self,
        creds: &lcu_client::LockfileCredentials,
        puuid: &str,
        count: usize,
    ) -> (String, Vec<MatchHistoryEntry>) {
        let sgp_enabled = self.get_settings().sgp_enabled;
        if sgp_enabled {
            if let Ok(raw) = self.lcu.fetch_sgp_match_history(creds, puuid, count).await {
                let list = parse_sgp_match_history(
                    &raw,
                    puuid,
                    |cid| {
                        self.champion_name_by_id(cid)
                            .unwrap_or_else(|| format!("英雄 #{cid}"))
                    },
                    |aid| self.augment_name_by_id(aid),
                );
                if !list.is_empty() {
                    return ("sgp".to_string(), list);
                }
            }
        }
        if let Ok(raw) = self
            .lcu
            .fetch_match_history_by_puuid(creds, puuid, count)
            .await
        {
            let list = parse_match_history(
                &raw,
                |cid| {
                    self.champion_name_by_id(cid)
                        .unwrap_or_else(|| format!("英雄 #{cid}"))
                },
                |aid| self.augment_name_by_id(aid),
            );
            return ("lcu".to_string(), list);
        }
        ("offline".to_string(), Vec::new())
    }

    pub async fn get_player_profile(
        &self,
        query_or_puuid: &str,
        count: usize,
    ) -> Result<PlayerProfile> {
        let summoner = self.search_player(query_or_puuid).await?;
        let encounter_summary = self
            .player_store
            .encounter_summary(&summoner.summoner_name);

        let (source, recent_matches) = if !summoner.puuid.is_empty()
            && !summoner.puuid.starts_with("local-")
        {
            if let Some(creds) = discover_lockfile() {
                self.fetch_matches_for_puuid(&creds, &summoner.puuid, count)
                    .await
            } else {
                ("offline".to_string(), Vec::new())
            }
        } else {
            ("local".to_string(), Vec::new())
        };

        let games = recent_matches.len();
        let wins = recent_matches.iter().filter(|m| m.result == "WIN").count();
        let win_rate = if games > 0 {
            wins as f64 / games as f64
        } else {
            0.0
        };
        let total_k: u32 = recent_matches.iter().map(|m| m.kills).sum();
        let total_d: u32 = recent_matches.iter().map(|m| m.deaths).sum();
        let total_a: u32 = recent_matches.iter().map(|m| m.assists).sum();
        let avg_kda = if games > 0 {
            format!(
                "{:.1} / {:.1} / {:.1}",
                total_k as f64 / games as f64,
                total_d as f64 / games as f64,
                total_a as f64 / games as f64
            )
        } else {
            "-".to_string()
        };
        let avg_damage = if games > 0 {
            recent_matches.iter().map(|m| m.damage).sum::<u64>() / games as u64
        } else {
            0
        };
        let avg_hex_score = if games > 0 {
            ((recent_matches.iter().map(|m| m.hex_score).sum::<f64>() / games as f64) * 10.0)
                .round()
                / 10.0
        } else {
            0.0
        };

        let mut champ_counts: HashMap<String, usize> = HashMap::new();
        for m in &recent_matches {
            *champ_counts.entry(m.hero_name.clone()).or_insert(0) += 1;
        }
        let mut top_champs: Vec<(String, usize)> = champ_counts.into_iter().collect();
        top_champs.sort_by(|a, b| b.1.cmp(&a.1));
        let top_champions = top_champs.into_iter().take(4).map(|(n, _)| n).collect();

        Ok(PlayerProfile {
            summoner,
            source,
            games_analyzed: games,
            win_rate,
            avg_kda,
            avg_damage,
            avg_hex_score,
            top_champions,
            recent_matches,
            encounter_summary,
        })
    }

    pub fn get_intel_cache_status(&self) -> IntelCacheStatus {
        let count = self.intel_cache.read().map(|g| g.len()).unwrap_or(0);
        let last_refresh = self.last_intel_refresh_epoch.read().ok().and_then(|g| *g);
        IntelCacheStatus {
            cached_players: count,
            sgp_enabled: self.get_settings().sgp_enabled,
            last_refresh_epoch: last_refresh,
            ttl_secs: INTEL_CACHE_TTL_SECS,
        }
    }

    pub async fn get_game_players(&self, force_refresh: bool) -> Vec<GamePlayerIntel> {
        let state = self.get_overlay_state();
        let now = now_epoch_secs();
        let creds = discover_lockfile();

        // Collect roster: prefer live_players, else gameflow session roster.
        let mut roster: Vec<(String, String, String, String)> = Vec::new();
        if !state.live_players.is_empty() {
            for p in &state.live_players {
                roster.push((
                    String::new(),
                    p.summoner_name.clone(),
                    p.champion_name.clone(),
                    p.team.clone(),
                ));
            }
        } else if let Some(ref c) = creds {
            if let Ok(session) = self.lcu.fetch_gameflow_session(c).await {
                if let Some(gd) = session.get("gameData") {
                    for (team_key, team_label) in [("teamOne", "ORDER"), ("teamTwo", "CHAOS")] {
                        if let Some(arr) = gd.get(team_key).and_then(|v| v.as_array()) {
                            for p in arr {
                                let puuid = p
                                    .get("puuid")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let name = p
                                    .get("summonerName")
                                    .or_else(|| p.get("gameName"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let cid = p
                                    .get("championId")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0) as u32;
                                let hero = self
                                    .champion_name_by_id(cid)
                                    .unwrap_or_else(|| format!("英雄 #{cid}"));
                                if !name.is_empty() {
                                    roster.push((puuid, name, hero, team_label.to_string()));
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut results = Vec::new();
        for (mut puuid, summoner_name, champion_name, team) in roster {
            let cache_key = summoner_name.to_lowercase();
            if !force_refresh {
                if let Ok(guard) = self.intel_cache.read() {
                    if let Some(entry) = guard.get(&cache_key) {
                        if now.saturating_sub(entry.fetched_at_epoch) <= INTEL_CACHE_TTL_SECS {
                            let mut item = entry.intel.clone();
                            item.champion_name = champion_name;
                            item.team = team;
                            item.marker = self.player_store.find_marker(&item.puuid, &summoner_name);
                            item.encounter_count = self
                                .player_store
                                .encounter_summary(&summoner_name)
                                .total_encounters;
                            results.push(item);
                            continue;
                        }
                    }
                }
            }

            let mut source = "local".to_string();
            let mut matches = Vec::new();
            if let Some(ref c) = creds {
                if puuid.is_empty() {
                    if let Ok(sum) = self.lcu.lookup_summoner(c, &summoner_name).await {
                        puuid = sum
                            .get("puuid")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                    }
                }
                if !puuid.is_empty() {
                    let (src, list) = self.fetch_matches_for_puuid(c, &puuid, 10).await;
                    source = src;
                    matches = list;
                }
            }

            let marker = self.player_store.find_marker(&puuid, &summoner_name);
            let encounter_count = self
                .player_store
                .encounter_summary(&summoner_name)
                .total_encounters;
            let recent_games = matches.len();
            let recent_win_rate = if recent_games > 0 {
                let w = matches.iter().filter(|m| m.result == "WIN").count();
                Some((w as f64 / recent_games as f64 * 1000.0).round() / 1000.0)
            } else {
                None
            };
            let avg_kda = if recent_games > 0 {
                let k: u32 = matches.iter().map(|m| m.kills).sum();
                let d: u32 = matches.iter().map(|m| m.deaths).sum();
                let a: u32 = matches.iter().map(|m| m.assists).sum();
                Some(format!(
                    "{:.1}/{:.1}/{:.1}",
                    k as f64 / recent_games as f64,
                    d as f64 / recent_games as f64,
                    a as f64 / recent_games as f64
                ))
            } else {
                None
            };
            let avg_hex_score = if recent_games > 0 {
                Some(
                    ((matches.iter().map(|m| m.hex_score).sum::<f64>() / recent_games as f64)
                        * 10.0)
                        .round()
                        / 10.0,
                )
            } else {
                None
            };

            let mut tags = Vec::new();
            if let Some(wr) = recent_win_rate {
                if wr >= 0.65 && recent_games >= 4 {
                    tags.push("连胜大腿".to_string());
                } else if wr <= 0.35 && recent_games >= 4 {
                    tags.push("近期连败".to_string());
                }
            }
            if let Some(ref m) = marker {
                tags.push(m.tag_label.clone());
            }
            if encounter_count > 0 {
                tags.push(format!("偶遇 {encounter_count} 局"));
            }

            let intel = GamePlayerIntel {
                puuid,
                summoner_name,
                champion_name,
                team,
                recent_games,
                recent_win_rate,
                avg_kda,
                avg_hex_score,
                tags,
                marker,
                encounter_count,
                source,
            };

            if let Ok(mut guard) = self.intel_cache.write() {
                guard.insert(
                    cache_key,
                    CachedIntelEntry {
                        intel: intel.clone(),
                        fetched_at_epoch: now,
                    },
                );
            }
            results.push(intel);
        }

        if force_refresh {
            if let Ok(mut ts) = self.last_intel_refresh_epoch.write() {
                *ts = Some(now);
            }
        }
        results
    }

    // -----------------------------------------------------------------------
    // LCU control, Ready Check Delay, Bench Grab & Cancel
    // -----------------------------------------------------------------------

    pub async fn probe_lcu(&self) -> LcuStatus {
        self.lcu.probe_status().await
    }

    pub async fn accept_ready_check(&self) -> Result<()> {
        let creds = discover_lockfile().ok_or_else(|| anyhow!("LeagueClient lockfile not found"))?;
        self.lcu.accept_ready_check(&creds).await
    }

    pub async fn bench_swap(&self, champion_id: u32) -> Result<()> {
        let creds = discover_lockfile().ok_or_else(|| anyhow!("LeagueClient lockfile not found"))?;
        self.lcu.bench_swap(&creds, champion_id).await
    }

    /// Arm a one-shot bench grab for `champion_id` (swaps immediately if already on bench).
    pub async fn grab_bench_champion(&self, champion_id: u32) -> BenchGrabStatus {
        let hero_name = self.champion_name_by_id(champion_id);
        if let Ok(mut c) = self.bench_autograb_cancelled.lock() {
            *c = false;
        }
        if let Some(creds) = discover_lockfile() {
            if self.lcu.bench_swap(&creds, champion_id).await.is_ok() {
                if let Ok(mut target) = self.pending_bench_grab.lock() {
                    *target = None;
                }
                return BenchGrabStatus {
                    active: false,
                    target_champion_id: Some(champion_id),
                    target_hero_name: hero_name.clone(),
                    status_text: format!(
                        "已直接抢下【{}】",
                        hero_name.as_deref().unwrap_or("目标英雄")
                    ),
                };
            }
        }
        if let Ok(mut target) = self.pending_bench_grab.lock() {
            *target = Some(champion_id);
        }
        BenchGrabStatus {
            active: true,
            target_champion_id: Some(champion_id),
            target_hero_name: hero_name.clone(),
            status_text: format!(
                "等待替补席出现【{}】，出现后将自动秒抢",
                hero_name.as_deref().unwrap_or("目标英雄")
            ),
        }
    }

    /// Cancel any armed bench grab and pause auto-grab for the current champ select session.
    pub fn cancel_bench_grab(&self) -> BenchGrabStatus {
        if let Ok(mut target) = self.pending_bench_grab.lock() {
            *target = None;
        }
        if let Ok(mut cancelled) = self.bench_autograb_cancelled.lock() {
            *cancelled = true;
        }
        BenchGrabStatus {
            active: false,
            target_champion_id: None,
            target_hero_name: None,
            status_text: "已取消本局自动抢英雄".to_string(),
        }
    }

    pub fn get_bench_grab_status(&self) -> BenchGrabStatus {
        let target = self.pending_bench_grab.lock().ok().and_then(|g| *g);
        let cancelled = self
            .bench_autograb_cancelled
            .lock()
            .ok()
            .map(|g| *g)
            .unwrap_or(false);
        if let Some(cid) = target {
            let name = self.champion_name_by_id(cid);
            BenchGrabStatus {
                active: true,
                target_champion_id: Some(cid),
                target_hero_name: name.clone(),
                status_text: format!("正在监听秒抢【{}】", name.as_deref().unwrap_or("目标英雄")),
            }
        } else if cancelled {
            BenchGrabStatus {
                active: false,
                target_champion_id: None,
                target_hero_name: None,
                status_text: "本局自动抢已暂停".to_string(),
            }
        } else {
            BenchGrabStatus {
                active: false,
                target_champion_id: None,
                target_hero_name: None,
                status_text: "未指定单独抢英雄目标".to_string(),
            }
        }
    }

    pub async fn poll_lcu_tick(&self) -> LcuPollOutcome {
        let status = self.lcu.probe_status().await;
        let settings = self.get_settings();
        let mut next_state = self.get_overlay_state();
        let prev_phase = next_state.lcu.phase.clone();
        let prev_connected = next_state.lcu.connected;
        let prev_dead = next_state.is_dead;
        let prev_hero = next_state.hero_name.clone();

        next_state.lcu = status.clone();
        next_state.phase = status.phase.clone();

        let mut auto_accepted = false;
        let mut auto_grabbed_champion_id = None;

        // Reset session-scoped flags when leaving their respective phases.
        if status.phase != "ReadyCheck" {
            if let Ok(mut seen) = self.ready_check_seen_at.lock() {
                *seen = None;
            }
        }
        if status.phase != "ChampSelect" {
            if let Ok(mut cancelled) = self.bench_autograb_cancelled.lock() {
                *cancelled = false;
            }
            if let Ok(mut last_spell) = self.last_auto_spell_champ_id.lock() {
                *last_spell = None;
            }
            // An armed grab target must not survive into a later game's lobby.
            if let Ok(mut target) = self.pending_bench_grab.lock() {
                *target = None;
            }
            if let Ok(mut is_aram) = self.champ_select_is_aram.lock() {
                *is_aram = None;
            }
        }

        if status.connected {
            if let Some(creds) = discover_lockfile() {
                if status.phase == "ReadyCheck" && settings.ready_check_auto_accept {
                    let ready_to_accept = {
                        let mut seen = self.ready_check_seen_at.lock().unwrap();
                        let started = seen.get_or_insert_with(Instant::now);
                        started.elapsed().as_millis() as u64 >= settings.ready_check_accept_delay_ms
                    };
                    if ready_to_accept && self.lcu.accept_ready_check(&creds).await.is_ok() {
                        auto_accepted = true;
                        if let Ok(mut seen) = self.ready_check_seen_at.lock() {
                            *seen = None;
                        }
                    }
                } else if status.phase == "ChampSelect" {
                    // Entering champ select: make sure the coming game starts
                    // borderless so overlays stay visible over it.
                    if settings.auto_fix_borderless && prev_phase != "ChampSelect" {
                        if let Ok(Some(mode)) = self.lcu.fetch_game_window_mode(&creds).await {
                            if mode != lcu_client::WINDOW_MODE_BORDERLESS {
                                let _ = self
                                    .lcu
                                    .set_game_window_mode(&creds, lcu_client::WINDOW_MODE_BORDERLESS)
                                    .await;
                            }
                        }
                    }
                    // Determine the queue once per champ-select so mode-specific
                    // automation (ARAM spell set) doesn't fire in other queues.
                    let cached_is_aram = self
                        .champ_select_is_aram
                        .lock()
                        .ok()
                        .and_then(|guard| *guard);
                    let is_aram_queue = match cached_is_aram {
                        Some(v) => v,
                        None => {
                            let detected = self.detect_aram_queue(&creds).await;
                            if let Ok(mut g) = self.champ_select_is_aram.lock() {
                                *g = Some(detected);
                            }
                            detected
                        }
                    };
                    if let Ok(cs) = self.lcu.fetch_champ_select(&creds).await {
                        if let Some(my_id) = cs.my_champion_id {
                            if let Some(name) = self.champion_name_by_id(my_id) {
                                next_state.hero_name = name.clone();
                                // Optional auto-apply ARAM summoner spells on
                                // champion change — ARAM queues only, so the
                                // snowball Mark set never lands in ranked.
                                if settings.auto_apply_summoner_spells && is_aram_queue {
                                    let should_apply = {
                                        let mut guard = self.last_auto_spell_champ_id.lock().unwrap();
                                        if *guard != Some(my_id) {
                                            *guard = Some(my_id);
                                            true
                                        } else {
                                            false
                                        }
                                    };
                                    if should_apply {
                                        let (s1, s2) = reco_engine::recommend_aram_spells(
                                            &self.dataset(),
                                            &name,
                                            &settings.summoner_flash_slot,
                                        );
                                        let _ = self.lcu.apply_summoner_spells(&creds, s1, s2).await;
                                    }
                                }
                            }
                        }
                        let allies: Vec<String> = cs
                            .ally_champion_ids
                            .iter()
                            .filter_map(|id| self.champion_name_by_id(*id))
                            .collect();
                        if !allies.is_empty() {
                            next_state.ally_team = allies;
                        }
                        let enemies: Vec<String> = cs
                            .enemy_champion_ids
                            .iter()
                            .filter_map(|id| self.champion_name_by_id(*id))
                            .collect();
                        if !enemies.is_empty() {
                            next_state.enemy_team = enemies;
                        }
                        let bench: Vec<String> = cs
                            .bench_champion_ids
                            .iter()
                            .filter_map(|id| self.champion_name_by_id(*id))
                            .collect();
                        next_state.bench_pool = bench;

                        // Priority 1: explicit armed target from `grab_bench_champion`
                        let armed_target = self.pending_bench_grab.lock().ok().and_then(|g| *g);
                        if let Some(target_id) = armed_target {
                            if cs.bench_champion_ids.contains(&target_id)
                                && cs.my_champion_id != Some(target_id)
                                && self.lcu.bench_swap(&creds, target_id).await.is_ok()
                            {
                                auto_grabbed_champion_id = Some(target_id);
                                if let Ok(mut t) = self.pending_bench_grab.lock() {
                                    *t = None;
                                }
                            }
                        }

                        // Priority 2: preferred bench auto-grab list (unless cancelled for this session)
                        let cancelled = self
                            .bench_autograb_cancelled
                            .lock()
                            .ok()
                            .map(|g| *g)
                            .unwrap_or(false);
                        if auto_grabbed_champion_id.is_none()
                            && settings.bench_autograb_enabled
                            && !cancelled
                        {
                            for preferred_id in &settings.bench_preferred_champion_ids {
                                if cs.bench_champion_ids.contains(preferred_id)
                                    && cs.my_champion_id != Some(*preferred_id)
                                {
                                    if self.lcu.bench_swap(&creds, *preferred_id).await.is_ok() {
                                        auto_grabbed_champion_id = Some(*preferred_id);
                                    }
                                    break;
                                }
                            }
                        }
                    }
                } else if status.phase == "InProgress" {
                    if let Ok(live) = self.lcu.fetch_live_game_snapshot().await {
                        if live.in_game {
                            if let Some(ref champ) = live.active_champion {
                                if !champ.is_empty() {
                                    next_state.hero_name = champ.clone();
                                }
                            }
                            next_state.current_gold = live.current_gold;
                            next_state.owned_item_ids = live.owned_item_ids.clone();
                            next_state.is_dead = live.is_dead;
                            if !live.allies.is_empty() {
                                next_state.ally_team = live.allies.clone();
                            }
                            if !live.enemies.is_empty() {
                                next_state.enemy_team = live.enemies.clone();
                            }
                            next_state.live_players = live.players.clone();
                            self.record_live_game_encounters(&live);
                        }
                    }
                }
            }
        }

        let changed = prev_connected != status.connected
            || prev_phase != status.phase
            || prev_dead != next_state.is_dead
            || prev_hero != next_state.hero_name
            || auto_accepted
            || auto_grabbed_champion_id.is_some();

        let saved = self.set_overlay_state(next_state);
        LcuPollOutcome {
            status,
            overlay_state: saved,
            changed,
            auto_accepted,
            auto_grabbed_champion_id,
        }
    }

    /// True when the current lobby is an ARAM(-family) queue. Reads
    /// `/lol-gameflow/v1/session` `gameData.queue` (id, name or gameMode);
    /// conservative: returns false when the information is unavailable.
    async fn detect_aram_queue(&self, creds: &lcu_client::LockfileCredentials) -> bool {
        let Ok(session) = self.lcu.fetch_gameflow_session(creds).await else {
            return false;
        };
        let Some(queue) = session.get("gameData").and_then(|g| g.get("queue")) else {
            return false;
        };
        let name = queue
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let game_mode = queue
            .get("gameMode")
            .and_then(|v| v.as_str())
            .or_else(|| {
                session
                    .get("gameData")
                    .and_then(|g| g.get("gameMode"))
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("")
            .to_ascii_lowercase();
        let queue_id = queue.get("id").and_then(|v| v.as_i64()).unwrap_or(-1);
        // 450 ARAM, 900 ARURF, 1900 ARURF snowdown, 2400 ARURF variant.
        matches!(queue_id, 450 | 900 | 1900 | 2400)
            || name.contains("aram")
            || name.contains("arurf")
            || game_mode.contains("aram")
            || game_mode.contains("urfsnow")
            || game_mode.contains("urf")
    }

    fn record_live_game_encounters(&self, live: &LiveGameSnapshot) {
        let players = &live.players;
        if players.is_empty() {
            return;
        }
        let mut sig_parts: Vec<String> = players
            .iter()
            .map(|p| format!("{}:{}", p.summoner_name, p.champion_name))
            .collect();
        sig_parts.sort();
        let game_sig = sig_parts.join("|");
        {
            let mut last = self.last_recorded_live_key.lock().unwrap();
            if last.as_deref() == Some(game_sig.as_str()) {
                return;
            }
            *last = Some(game_sig.clone());
        }

        // Identify ourselves from liveclientdata's activePlayer — the LCU
        // `current-summoner` name format (with/without #tag) differs from the
        // in-game name, so comparing against it can fail to skip ourselves.
        let my_name_lower = normalize_summoner_name(&live.active_summoner_name);
        // Fall back to the parsed team field rather than assuming ORDER; if we
        // can't determine our team at all, skip recording rather than invert
        // ally/enemy for everyone.
        let my_team = players
            .iter()
            .find(|p| !my_name_lower.is_empty() && normalize_summoner_name(&p.summoner_name) == my_name_lower)
            .map(|p| p.team.as_str())
            .or(if live.active_team.is_empty() {
                None
            } else {
                Some(live.active_team.as_str())
            });
        let Some(my_team) = my_team else {
            return;
        };

        let ts = now_epoch_secs();
        let records: Vec<PlayerEncounterRecord> = players
            .iter()
            .filter(|p| {
                !p.summoner_name.is_empty()
                    && (my_name_lower.is_empty()
                        || normalize_summoner_name(&p.summoner_name) != my_name_lower)
            })
            .map(|p| PlayerEncounterRecord {
                id: format!("live-{ts}-{}", normalize_summoner_name(&p.summoner_name)),
                puuid: String::new(),
                summoner_name: p.summoner_name.clone(),
                champion_name: p.champion_name.clone(),
                relation: if p.team == my_team { "ally" } else { "enemy" }.to_string(),
                result: "UNKNOWN".to_string(),
                kda: format!("{}/{}/{}", p.kills, p.deaths, p.assists),
                timestamp: ts,
                time_label: "本局遭遇".to_string(),
            })
            .collect();

        let _ = self.player_store.record_encounters(records);
    }

    // -----------------------------------------------------------------------
    // OCR Execution
    // -----------------------------------------------------------------------

    pub fn resolve_ocr_python(&self) -> PathBuf {
        resolve_ocr_python_from_root(&self.root)
    }

    fn resolve_image_path(&self, image_path: Option<&str>) -> Option<String> {
        match image_path {
            Some(p) if !p.is_empty() && Path::new(p).exists() => Some(p.to_string()),
            Some(p) if !p.is_empty() => {
                let rel = self.root.join(p);
                if rel.exists() {
                    Some(rel.display().to_string())
                } else {
                    None
                }
            }
            _ => {
                let sample = resolve_ocr_sidecar_dir(&self.root).join("sample_augment_1080p.png");
                if sample.exists() {
                    Some(sample.display().to_string())
                } else {
                    None
                }
            }
        }
    }

    pub fn run_ocr_preset(
        &self,
        preset_key: &str,
        image_path: Option<&str>,
    ) -> Result<Value> {
        let settings = self.get_settings();
        let lang = self.current_ocr_lang();
        let resolved_img = self.resolve_image_path(image_path);
        let mut worker = self
            .ocr_worker
            .lock()
            .map_err(|_| anyhow!("ocr worker lock poisoned"))?;
        let resp = worker.run_pipeline(
            preset_key,
            resolved_img.as_deref(),
            &settings.ocr_performance_mode,
            lang,
            true,
        )?;
        self.apply_ocr_response_to_overlay(&resp);
        Ok(resp)
    }

    pub fn ocr_watch_tick(
        &self,
        preset_key: &str,
        image_path: Option<&str>,
    ) -> Result<Value> {
        let settings = self.get_settings();
        let lang = self.current_ocr_lang();
        let resolved_img = match image_path {
            Some(p) if !p.is_empty() => self.resolve_image_path(Some(p)),
            _ => None, // live screen capture when no file path is passed
        };
        let mut worker = self
            .ocr_worker
            .lock()
            .map_err(|_| anyhow!("ocr worker lock poisoned"))?;
        let resp = worker.watch_tick(
            preset_key,
            resolved_img.as_deref(),
            &settings.ocr_performance_mode,
            lang,
            settings.allow_non_lol_foreground,
        )?;
        self.apply_ocr_response_to_overlay(&resp);
        Ok(resp)
    }

    pub fn reset_ocr_watcher(&self) -> Result<Value> {
        let mut worker = self
            .ocr_worker
            .lock()
            .map_err(|_| anyhow!("ocr worker lock poisoned"))?;
        worker.reset_watcher()
    }

    fn apply_ocr_response_to_overlay(&self, resp: &Value) {
        if resp.get("status").and_then(|v| v.as_str()) != Some("ok") {
            return;
        }
        let matches: Vec<String> = resp
            .get("ocr_matches")
            .or_else(|| resp.get("result"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        item.get("match_name")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(ToString::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();

        let ocr_ms = resp
            .get("timings")
            .and_then(|t| t.get("ocr_ms"))
            .and_then(|v| v.as_f64());

        let mut offer_changed = false;
        if let Ok(mut guard) = self.overlay_state.write() {
            if matches.len() == 3 {
                if guard.candidate_augments != matches {
                    offer_changed = true;
                }
                guard.candidate_augments = matches.clone();
                guard.ocr_status = Some(format!("常驻 OCR 识别成功：{}", matches.join(" / ")));
            }
            guard.ocr_ms = ocr_ms;
        }
        if offer_changed {
            self.player_store.record_augment_offer(&matches);
        }
    }
}

/// Roots that may hold `ocr-sidecar/` and `models/`: the app root, the folder
/// next to the executable (portable / installed layouts), and the source
/// checkout this binary was built from (dev runs).
fn search_roots(root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![root.to_path_buf(), root.join("resources")];
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        roots.push(exe_dir.clone());
        roots.push(exe_dir.join("resources"));
    }
    if let Some(workspace) = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
    {
        roots.push(workspace.to_path_buf());
    }
    roots
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Directory containing `worker.py`. Falls back to `<root>/ocr-sidecar` so the
/// "not found" error names the place the user is expected to put it.
pub fn resolve_ocr_sidecar_dir(root: &Path) -> PathBuf {
    if let Some(dir) = env_path("HEXBOX_OCR_SIDECAR_DIR") {
        if dir.join("worker.py").exists() {
            return dir;
        }
    }
    search_roots(root)
        .into_iter()
        .map(|r| r.join("ocr-sidecar"))
        .find(|dir| dir.join("worker.py").exists())
        .unwrap_or_else(|| root.join("ocr-sidecar"))
}

pub fn resolve_model_dir(root: &Path) -> PathBuf {
    if let Some(dir) = env_path("ARAM_ASSISTANT_OCR_MODEL_DIR") {
        if dir.is_dir() {
            return dir;
        }
    }
    let dirs: Vec<PathBuf> = search_roots(root)
        .into_iter()
        .map(|r| r.join("models"))
        .collect();
    // Prefer a folder that really has model weights over one with only dictionaries.
    dirs.iter()
        .find(|dir| has_onnx_model(dir))
        .or_else(|| dirs.iter().find(|dir| dir.is_dir()))
        .cloned()
        .unwrap_or_else(|| root.join("models"))
}

/// Compiled runner shipped with the installer, if present.
pub fn resolve_compiled_ocr_runner(root: &Path) -> Option<PathBuf> {
    let exe_name = if cfg!(windows) { "hexbox-ocr.exe" } else { "hexbox-ocr" };
    if let Some(exe) = env_path("HEXBOX_OCR_RUNNER") {
        if exe.is_file() {
            return Some(exe);
        }
    }
    search_roots(root)
        .into_iter()
        .map(|r| r.join("ocr").join(exe_name))
        .find(|exe| exe.is_file())
}

/// The compiled runner wins when it exists; otherwise fall back to the Python
/// sources. `HEXBOX_OCR_USE_PYTHON=1` forces the Python path (handy in dev).
pub fn resolve_ocr_launcher(root: &Path) -> OcrLauncher {
    let force_python = env_path("HEXBOX_OCR_USE_PYTHON").is_some();
    if !force_python {
        if let Some(exe) = resolve_compiled_ocr_runner(root) {
            return OcrLauncher::Compiled(exe);
        }
    }
    OcrLauncher::Python {
        python: resolve_ocr_python_from_root(root),
        script: resolve_ocr_sidecar_dir(root).join("worker.py"),
    }
}

fn has_onnx_model(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .any(|e| e.path().extension().is_some_and(|ext| ext == "onnx"))
        })
        .unwrap_or(false)
}

fn resolve_ocr_python_from_root(root: &Path) -> PathBuf {
    if let Some(python) = env_path("ARAM_ASSISTANT_OCR_PYTHON") {
        return python;
    }
    for base in search_roots(root) {
        let candidates = [
            base.join("ocr-sidecar/.venv/Scripts/python.exe"),
            base.join("ocr-sidecar/.venv/bin/python"),
            base.join(".venv/Scripts/python.exe"),
            base.join(".venv/bin/python"),
            base.join("runtime/python.exe"),
        ];
        for cand in candidates {
            if cand.exists() {
                return cand;
            }
        }
    }
    // `python3` on Windows is usually the Microsoft Store stub, not a real interpreter.
    PathBuf::from(if cfg!(windows) { "python" } else { "python3" })
}

fn load_settings_file(data_dir: &Path) -> AppSettings {
    let path = data_dir.join("user_settings.json");
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(parsed) = serde_json::from_str::<AppSettings>(&raw) {
            return parsed;
        }
    }
    AppSettings::default()
}

fn save_settings_file(data_dir: &Path, settings: &AppSettings) -> Result<()> {
    let _ = fs::create_dir_all(data_dir);
    let path = data_dir.join("user_settings.json");
    let serialized = serde_json::to_string_pretty(settings)?;
    fs::write(path, serialized)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn initializes_core_from_workspace() {
        let root = workspace_root();
        let core = HexBoxCore::initialize(&root).expect("initialize HexBoxCore");
        let summary = core.runtime_summary();
        if root.join("data/champions.json").exists() {
            assert!(summary.hero_count >= 170);
            assert!(summary.augment_count >= 200);
            assert_eq!(summary.mechanic_link_count, 33);
            assert_eq!(summary.mechanic_conflict_count, 4);
        }
    }

    #[test]
    fn persistent_ocr_worker_recognizes_sample_image() {
        let root = workspace_root();
        let sample = root.join("ocr-sidecar/sample_augment_1080p.png");
        let venv_py = root.join("ocr-sidecar/.venv/bin/python");
        // Skip unless a runnable OCR engine *and* its inputs exist; CI builds
        // the runner but `models/*.onnx` and `data/` are gitignored.
        let model_dir = resolve_model_dir(&root);
        let has_models = model_dir.join("ch_PP-OCRv4_rec_int8.onnx").exists()
            && model_dir.join("ppocr_keys_v1.txt").exists();
        let has_dict = root.join("data/augments_full.json").exists()
            || root.join("data/augments.json").exists();
        let can_run = venv_py.exists() || resolve_compiled_ocr_runner(&root).is_some();
        if !sample.exists() || !can_run || !has_models || !has_dict {
            return;
        }
        let core = HexBoxCore::initialize(&root).expect("initialize HexBoxCore");
        let res = core
            .run_ocr_preset("1920x1080@100", Some(sample.to_str().unwrap()))
            .expect("run persistent ocr worker");
        assert_eq!(res.get("status").and_then(|v| v.as_str()), Some("ok"));
        let state = core.get_overlay_state();
        assert_eq!(state.candidate_augments.len(), 3);
        assert_eq!(state.candidate_augments[0], "亮出你的剑");
    }

    #[test]
    fn sidecar_is_found_when_app_root_has_no_ocr_sidecar_folder() {
        // An installed app whose root holds only data/ must still find worker.py.
        let empty_root = std::env::temp_dir().join("hexbox-root-without-sidecar");
        let _ = fs::create_dir_all(&empty_root);
        let dir = resolve_ocr_sidecar_dir(&empty_root);
        assert!(dir.join("worker.py").exists(), "resolved {}", dir.display());
    }

    #[test]
    fn manual_override_and_bench_cancel_work() {
        let root = workspace_root();
        let core = HexBoxCore::initialize(&root).expect("initialize HexBoxCore");
        let updated = core
            .set_manual_augment_override(2, "尤里卡".to_string())
            .expect("override slot 2");
        assert_eq!(updated.candidate_augments[1], "尤里卡");

        let selected = core.set_manual_augment_selection(vec!["易损".into(), "尤里卡".into()]);
        assert_eq!(selected.confirmed_augments.len(), 2);
        assert_eq!(selected.stage, 3);

        let cancelled = core.cancel_bench_grab();
        assert!(!cancelled.active);
        assert!(core.get_bench_grab_status().status_text.contains("暂停"));
    }

    #[test]
    fn set_overlay_state_if_changed_ignores_identical_echo() {
        let root = workspace_root();
        let core = HexBoxCore::initialize(&root).expect("initialize HexBoxCore");
        let mut payload = core.get_overlay_state();
        payload.ocr_status = Some("常驻 OCR 识别成功：亮出你的剑 / 风语者的祝福 / 男爵之手".into());
        payload.ocr_ms = Some(72.3);

        let (saved, changed) = core.set_overlay_state_if_changed(payload.clone());
        assert!(changed);
        assert_eq!(saved.ocr_status, payload.ocr_status);

        let (_, changed_again) = core.set_overlay_state_if_changed(payload);
        assert!(!changed_again);
    }
}
