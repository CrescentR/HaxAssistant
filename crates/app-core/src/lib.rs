pub mod ocr_worker;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use anyhow::{anyhow, Result};
use data_runtime::Dataset;
use data_updater::{verify_manifest, ManifestReport};
use lcu_client::{discover_lockfile, parse_match_history, LcuClient};
use ocr_worker::PersistentOcrWorker;
use serde_json::Value;
use shared_types::{
    AppSettings, AugmentRecommendationCard, EnemyCounterAnalysis, LcuStatus, LineupEvaluation,
    MatchHistoryResponse, OverlayStatePayload, RuntimeSummary,
};

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
    pub dataset: Dataset,
    pub lcu: LcuClient,
    settings: RwLock<AppSettings>,
    overlay_state: RwLock<OverlayStatePayload>,
    ocr_worker: Mutex<PersistentOcrWorker>,
}

impl HexBoxCore {
    pub fn initialize(root: &Path) -> Result<Self> {
        let dataset = Dataset::load_from_root(root)?;
        let settings = load_settings_file(&dataset.data_dir);
        let python = resolve_ocr_python_from_root(root);
        let script = root.join("ocr-sidecar/worker.py");
        let model_dir = root.join("models");
        let ocr_worker = PersistentOcrWorker::new(python, script, dataset.data_dir.clone(), model_dir);

        Ok(Self {
            root: root.to_path_buf(),
            dataset,
            lcu: LcuClient::new(),
            settings: RwLock::new(settings),
            overlay_state: RwLock::new(OverlayStatePayload::default()),
            ocr_worker: Mutex::new(ocr_worker),
        })
    }

    pub fn runtime_summary(&self) -> RuntimeSummary {
        RuntimeSummary {
            patch: self.dataset.patch().to_string(),
            report_date: self.dataset.report_date().to_string(),
            hero_count: self.dataset.champions.len(),
            augment_count: self.dataset.augments_full.len(),
            mechanic_link_count: self.dataset.mechanic_pairs.links.len(),
            mechanic_conflict_count: self.dataset.mechanic_pairs.conflicts.len(),
            data_root: self.dataset.data_dir.display().to_string(),
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
        save_settings_file(&self.dataset.data_dir, &next)?;
        Ok(next)
    }

    pub fn get_overlay_state(&self) -> OverlayStatePayload {
        self.overlay_state
            .read()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    pub fn set_overlay_state(&self, next: OverlayStatePayload) -> OverlayStatePayload {
        if let Ok(mut guard) = self.overlay_state.write() {
            *guard = next.clone();
        }
        next
    }

    pub fn champion_name_by_id(&self, id: u32) -> Option<String> {
        let id_str = id.to_string();
        self.dataset
            .champions
            .iter()
            .find(|c| c.id == id_str)
            .map(|c| c.hero.clone())
    }

    pub fn augment_name_by_id(&self, id: u32) -> Option<String> {
        self.dataset
            .augments_full
            .iter()
            .find(|a| a.augment_id == id)
            .map(|a| a.augment_name.clone())
    }

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

    pub fn verify_data_manifest(&self) -> Result<ManifestReport> {
        verify_manifest(&self.dataset.data_dir)
    }

    pub fn evaluate_augment_offer(
        &self,
        hero_name: &str,
        candidate_names: &[String],
        stage: u8,
        confirmed_augments: &[String],
    ) -> Vec<AugmentRecommendationCard> {
        reco_engine::evaluate_augment_offer(
            &self.dataset,
            hero_name,
            candidate_names,
            stage,
            confirmed_augments,
        )
    }

    pub fn analyze_enemy_counter(
        &self,
        ally_hero_name: &str,
        enemy_hero_names: &[String],
    ) -> EnemyCounterAnalysis {
        reco_engine::analyze_enemy_counter(&self.dataset, ally_hero_name, enemy_hero_names)
    }

    pub fn evaluate_5v5_lineup(
        &self,
        ally_names: &[String],
        enemy_names: &[String],
        bench_names: &[String],
        local_hero_name: &str,
    ) -> LineupEvaluation {
        reco_engine::evaluate_5v5_lineup(
            &self.dataset,
            ally_names,
            enemy_names,
            bench_names,
            local_hero_name,
        )
    }

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

        if status.connected {
            if let Some(creds) = discover_lockfile() {
                if status.phase == "ReadyCheck" && settings.ready_check_auto_accept {
                    if self.lcu.accept_ready_check(&creds).await.is_ok() {
                        auto_accepted = true;
                    }
                } else if status.phase == "ChampSelect" {
                    if let Ok(cs) = self.lcu.fetch_champ_select(&creds).await {
                        if let Some(my_id) = cs.my_champion_id {
                            if let Some(name) = self.champion_name_by_id(my_id) {
                                next_state.hero_name = name;
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

                        if settings.bench_autograb_enabled {
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
                            if let Some(champ) = live.active_champion {
                                if !champ.is_empty() {
                                    next_state.hero_name = champ;
                                }
                            }
                            next_state.current_gold = live.current_gold;
                            next_state.is_dead = live.is_dead;
                            if !live.allies.is_empty() {
                                next_state.ally_team = live.allies;
                            }
                            if !live.enemies.is_empty() {
                                next_state.enemy_team = live.enemies;
                            }
                            next_state.live_players = live.players;
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
                let sample = self.root.join("ocr-sidecar/sample_augment_1080p.png");
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
        let resolved_img = self.resolve_image_path(image_path);
        let mut worker = self
            .ocr_worker
            .lock()
            .map_err(|_| anyhow!("ocr worker lock poisoned"))?;
        let resp = worker.run_pipeline(
            preset_key,
            resolved_img.as_deref(),
            &settings.ocr_performance_mode,
        )?;
        self.apply_ocr_response_to_overlay(&resp);
        Ok(resp)
    }

    pub fn ocr_watch_tick(
        &self,
        preset_key: &str,
        image_path: Option<&str>,
    ) -> Result<Value> {
        let resolved_img = match image_path {
            Some(p) if !p.is_empty() => self.resolve_image_path(Some(p)),
            _ => None, // live screen capture when no file path is passed
        };
        let mut worker = self
            .ocr_worker
            .lock()
            .map_err(|_| anyhow!("ocr worker lock poisoned"))?;
        let resp = worker.watch_tick(preset_key, resolved_img.as_deref())?;
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

        if let Ok(mut guard) = self.overlay_state.write() {
            if matches.len() == 3 {
                guard.candidate_augments = matches.clone();
                guard.ocr_status = Some(format!("常驻 OCR 识别成功：{}", matches.join(" / ")));
            }
            guard.ocr_ms = ocr_ms;
        }
    }
}

fn resolve_ocr_python_from_root(root: &Path) -> PathBuf {
    let candidates = [
        root.join("ocr-sidecar/.venv/bin/python"),
        root.join(".venv/bin/python"),
        root.join("ocr-sidecar/.venv/Scripts/python.exe"),
    ];
    for cand in candidates {
        if cand.exists() {
            return cand;
        }
    }
    PathBuf::from("python3")
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
        if !sample.exists() || !venv_py.exists() {
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
}
