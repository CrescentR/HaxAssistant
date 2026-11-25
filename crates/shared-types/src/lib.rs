pub mod hex_score;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use hex_score::{get_tier_meta, TierMeta};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub appearance_mode: String,
    pub augment_hotkey: String,
    pub item_overlay_hotkey: String,
    pub auto_show_overlay: bool,
    pub auto_show_formula_item_overlay: bool,
    pub player_scores_overlay_enabled: bool,
    pub death_shop_hint_enabled: bool,
    pub bench_quick_grab_enabled: bool,
    pub bench_autograb_enabled: bool,
    pub bench_preferred_champion_ids: Vec<u32>,
    pub auto_ocr_enabled: bool,
    pub ocr_performance_mode: String,
    pub selected_preset_key: String,
    pub show_augment_stats: bool,
    pub sgp_enabled: bool,
    pub ready_check_auto_accept: bool,
    #[serde(default)]
    pub ready_check_accept_delay_ms: u64,
    #[serde(default = "default_flash_slot")]
    pub summoner_flash_slot: String,
    #[serde(default)]
    pub auto_apply_summoner_spells: bool,
    #[serde(default = "default_augment_value_mode")]
    pub augment_value_mode: String,
    #[serde(default)]
    pub data_sync_url: String,
    #[serde(default)]
    pub allow_non_lol_foreground: bool,
    pub overlay_opacity: u8,
    pub overlay_zoom: u16,
}

fn default_flash_slot() -> String {
    "F".to_string()
}

fn default_augment_value_mode() -> String {
    "balanced".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            appearance_mode: "dark".to_string(),
            augment_hotkey: "F8".to_string(),
            item_overlay_hotkey: "F7".to_string(),
            auto_show_overlay: true,
            auto_show_formula_item_overlay: true,
            player_scores_overlay_enabled: true,
            death_shop_hint_enabled: true,
            bench_quick_grab_enabled: true,
            bench_autograb_enabled: true,
            bench_preferred_champion_ids: vec![157, 4, 114, 51, 22],
            auto_ocr_enabled: true,
            ocr_performance_mode: "balanced".to_string(),
            selected_preset_key: "1920x1080@100".to_string(),
            show_augment_stats: true,
            sgp_enabled: true,
            ready_check_auto_accept: true,
            ready_check_accept_delay_ms: 0,
            summoner_flash_slot: default_flash_slot(),
            auto_apply_summoner_spells: false,
            augment_value_mode: default_augment_value_mode(),
            data_sync_url: String::new(),
            allow_non_lol_foreground: false,
            overlay_opacity: 95,
            overlay_zoom: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChampionSummary {
    #[serde(default)]
    pub id: String,
    pub hero: String,
    #[serde(default)]
    pub full_name: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub search_terms: Option<Vec<String>>,
    #[serde(default = "default_half")]
    pub win_rate: f64,
    #[serde(default)]
    pub pick_rate: f64,
    #[serde(default)]
    pub games: u64,
    #[serde(default)]
    pub wins: Option<u64>,
    #[serde(default = "default_hex")]
    pub hex_score: f64,
    #[serde(default)]
    pub hex_tier: Option<String>,
    #[serde(default = "default_label")]
    pub hex_label: String,
    #[serde(default = "default_color")]
    pub hex_tier_color: String,
    #[serde(default)]
    pub kda: Option<f64>,
    #[serde(default)]
    pub avg_damage: Option<f64>,
    #[serde(default)]
    pub avg_damage_taken: Option<f64>,
    #[serde(default)]
    pub avg_heal_shield: Option<f64>,
    #[serde(default)]
    pub avg_cc_time: Option<f64>,
}

fn default_half() -> f64 {
    0.5
}

fn default_hex() -> f64 {
    55.0
}

fn default_label() -> String {
    "人上人".to_string()
}

fn default_color() -> String {
    "#b08a2d".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AugmentCatalogEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub augment_id: u32,
    pub augment_name: String,
    #[serde(default)]
    pub normalized_name: String,
    #[serde(default)]
    pub english_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub rarity: String,
    #[serde(default)]
    pub tier: u8,
    #[serde(default)]
    pub games: u64,
    #[serde(default = "default_half")]
    pub win_rate: f64,
    #[serde(default)]
    pub pick_rate: f64,
    #[serde(default = "default_hex")]
    pub hex_score: f64,
    #[serde(default)]
    pub hex_tier: String,
    #[serde(default)]
    pub hex_label: String,
    #[serde(default)]
    pub hex_tier_color: String,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AugmentTopItem {
    pub item_id: Value,
    pub item_name: String,
    pub win_rate: f64,
    pub average_index: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeroAugmentStatRow {
    #[serde(default)]
    pub augment_id: u32,
    pub augment_name: String,
    #[serde(default)]
    pub rarity: String,
    #[serde(default = "default_hex")]
    pub hex_score: f64,
    #[serde(default = "default_half")]
    pub win_rate: f64,
    #[serde(default)]
    pub pick_rate: f64,
    #[serde(default)]
    pub delta_win_rate: f64,
    #[serde(default)]
    pub games: u64,
    #[serde(default)]
    pub top_items: Vec<AugmentTopItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AugmentMechanicLink {
    #[serde(default)]
    pub a_name: String,
    #[serde(default)]
    pub b_name: String,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictTarget {
    #[serde(default)]
    pub id: Value,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AugmentMechanicConflict {
    #[serde(default)]
    pub a_name: String,
    #[serde(default)]
    pub rule: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub bs: Vec<ConflictTarget>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AugmentMechanicPairs {
    #[serde(default)]
    pub schema_version: String,
    #[serde(default)]
    pub reviewed_patch: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub links: Vec<AugmentMechanicLink>,
    #[serde(default)]
    pub conflicts: Vec<AugmentMechanicConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AugmentRecommendationCard {
    pub slot: usize,
    pub hero_name: String,
    pub augment_id: u32,
    pub augment_name: String,
    pub rarity: String,
    pub hex_score: f64,
    pub hex_tier: String,
    pub hex_label: String,
    pub hex_tier_color: String,
    pub win_rate: f64,
    pub pick_rate: f64,
    pub delta_win_rate: f64,
    pub sample_size: u64,
    pub stage_shift: f64,
    pub reason: String,
    pub synergy_note: Option<String>,
    pub conflict_note: Option<String>,
    pub top_items: Vec<AugmentTopItem>,
    pub is_best: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnemyCounterItemSuggestion {
    pub item_id: u32,
    pub item_name: String,
    pub reason: String,
    pub lift: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnemyCounterAnalysis {
    pub state: String,
    pub lean: String,
    pub physical_share: f64,
    pub magic_share: f64,
    pub true_share: f64,
    pub boots: Option<EnemyCounterItemSuggestion>,
    pub anti_heal: Option<EnemyCounterItemSuggestion>,
    pub defensive_items: Vec<EnemyCounterItemSuggestion>,
    pub healers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormulaItemRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaItemBuild {
    pub hero_id: String,
    pub hero_name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub starter_items: Vec<FormulaItemRef>,
    #[serde(default)]
    pub core_items: Vec<FormulaItemRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchSuggestion {
    pub hero_name: String,
    pub hero_id: String,
    pub hero_win_rate: f64,
    pub hex_score: f64,
    pub hex_label: String,
    pub hex_color: String,
    pub gain: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineupEvaluation {
    pub base_win: f64,
    pub adjusted_win: f64,
    pub correction: f64,
    pub notes: Vec<String>,
    pub bench_suggestions: Vec<BenchSuggestion>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LcuStatus {
    pub connected: bool,
    pub phase: String,
    pub port: Option<u16>,
    pub summoner_name: Option<String>,
    pub lockfile_path: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampSelectSnapshot {
    pub my_champion_id: Option<u32>,
    pub ally_champion_ids: Vec<u32>,
    pub enemy_champion_ids: Vec<u32>,
    pub bench_champion_ids: Vec<u32>,
    pub timer_phase: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LivePlayerScore {
    pub summoner_name: String,
    pub champion_name: String,
    pub team: String,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub is_dead: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveGameSnapshot {
    pub in_game: bool,
    pub active_champion: Option<String>,
    pub current_gold: f64,
    pub is_dead: bool,
    pub game_time: f64,
    /// Item ids in the active player's inventory.
    #[serde(default)]
    pub owned_item_ids: Vec<u32>,
    pub allies: Vec<String>,
    pub enemies: Vec<String>,
    pub players: Vec<LivePlayerScore>,
    /// Team of the active player ("ORDER" or "CHAOS"); empty if undetermined.
    #[serde(default)]
    pub active_team: String,
    /// Summoner name of the active player as reported by liveclientdata.
    #[serde(default)]
    pub active_summoner_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayStatePayload {
    pub hero_name: String,
    pub phase: String,
    pub stage: u8,
    pub candidate_augments: Vec<String>,
    pub confirmed_augments: Vec<String>,
    pub ally_team: Vec<String>,
    pub enemy_team: Vec<String>,
    pub bench_pool: Vec<String>,
    pub current_gold: f64,
    #[serde(default)]
    pub owned_item_ids: Vec<u32>,
    pub is_dead: bool,
    pub lcu: LcuStatus,
    pub live_players: Vec<LivePlayerScore>,
    pub ocr_status: Option<String>,
    pub ocr_ms: Option<f64>,
}

impl Default for OverlayStatePayload {
    fn default() -> Self {
        Self {
            hero_name: "寒冰射手".to_string(),
            phase: "Disconnected".to_string(),
            stage: 2,
            candidate_augments: vec![
                "亮出你的剑".to_string(),
                "暴击飞弹".to_string(),
                "最终形态".to_string(),
            ],
            confirmed_augments: vec!["易损".to_string()],
            ally_team: vec![
                "寒冰射手".to_string(),
                "光辉女郎".to_string(),
                "探险家".to_string(),
                "皮城女警".to_string(),
                "深海泰坦".to_string(),
            ],
            enemy_team: vec![
                "疾风剑豪".to_string(),
                "暗裔剑魔".to_string(),
                "祖安狂人".to_string(),
                "众星之子".to_string(),
                "影流之主".to_string(),
            ],
            bench_pool: vec![
                "曙光女神".to_string(),
                "卡牌大师".to_string(),
                "无双剑姬".to_string(),
                "虚空女皇".to_string(),
            ],
            current_gold: 2850.0,
            owned_item_ids: Vec::new(),
            is_dead: false,
            lcu: LcuStatus {
                connected: false,
                phase: "Disconnected".to_string(),
                port: None,
                summoner_name: None,
                lockfile_path: None,
                locale: None,
            },
            live_players: Vec::new(),
            ocr_status: None,
            ocr_ms: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchHistoryEntry {
    pub game_id: String,
    pub champion_id: u32,
    pub hero_name: String,
    pub result: String,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub kda: String,
    pub damage: u64,
    #[serde(default)]
    pub damage_taken: u64,
    #[serde(default)]
    pub heal: u64,
    /// Final inventory item ids (empty slots omitted).
    #[serde(default)]
    pub items: Vec<u32>,
    pub duration_secs: u64,
    pub duration: String,
    pub queue_id: u32,
    pub game_mode: String,
    pub augments: Vec<String>,
    pub hex_score: f64,
    pub game_creation: u64,
    pub time_ago: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchHistoryResponse {
    pub source: String,
    pub summoner_name: Option<String>,
    pub matches: Vec<MatchHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSummary {
    pub patch: String,
    pub report_date: String,
    pub hero_count: usize,
    pub augment_count: usize,
    pub mechanic_link_count: usize,
    pub mechanic_conflict_count: usize,
    pub data_root: String,
}

// ---------------------------------------------------------------------------
// Item prices and purchase plan
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ItemPrice {
    #[serde(default)]
    pub name: String,
    /// Full price including every component.
    #[serde(default)]
    pub total: u32,
    /// Recipe (combine) cost on top of the components.
    #[serde(default)]
    pub base: u32,
    /// Direct component item ids.
    #[serde(default)]
    pub from: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPlanStep {
    pub item_id: String,
    pub item_name: String,
    /// `starter`, `core` or `counter`.
    pub kind: String,
    pub total_cost: u32,
    /// Gold still needed, after crediting components already owned.
    pub remaining_cost: u32,
    pub owned: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPurchase {
    pub item_id: String,
    pub item_name: String,
    pub cost: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPlan {
    pub hero_name: String,
    pub current_gold: u32,
    /// Full build order; owned steps stay in the list, flagged `owned`.
    pub steps: Vec<ItemPlanStep>,
    /// First step not yet owned.
    pub next_item: Option<ItemPlanStep>,
    /// What to buy right now with `current_gold`, in order.
    pub buy_now: Vec<ItemPurchase>,
    pub gold_after: u32,
    /// Gold still missing to finish `next_item` after `buy_now`.
    pub gold_short: u32,
    pub note: String,
}

// ---------------------------------------------------------------------------
// Post-game review
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HeroBenchmark {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub games: u64,
    /// Per-minute values at each quantile of `HeroBenchmarks::quantiles`.
    #[serde(default)]
    pub damage: Vec<f64>,
    #[serde(default)]
    pub taken: Vec<f64>,
    #[serde(default)]
    pub heal: Vec<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AugmentTrio {
    #[serde(default)]
    pub augments: Vec<String>,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub win_rate: f64,
    #[serde(default)]
    pub hex_score: f64,
    #[serde(default)]
    pub games: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostgameMetric {
    /// `damage`, `taken` or `heal`.
    pub key: String,
    pub label: String,
    pub per_minute: f64,
    /// 0..1 rank among games on this hero; `None` without benchmark data.
    pub percentile: Option<f64>,
    pub median: Option<f64>,
    pub verdict: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostgameAugmentVerdict {
    pub name: String,
    pub hex_score: f64,
    pub tier_label: String,
    pub tier_color: String,
    pub win_rate: f64,
    pub games: u64,
    /// False when the hero has no statistics for this augment.
    pub hero_specific: bool,
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostgameReview {
    pub game_id: String,
    pub hero_name: String,
    pub result: String,
    /// S / A / B / C / D.
    pub grade: String,
    pub score: f64,
    pub metrics: Vec<PostgameMetric>,
    /// False for games under 10 minutes, where per-minute benchmarks are unreliable.
    pub benchmark_reliable: bool,
    pub augments: Vec<PostgameAugmentVerdict>,
    pub links: Vec<String>,
    pub conflicts: Vec<String>,
    pub matched_trio: Option<String>,
    /// Higher-rated augments for this hero that were not taken.
    pub better_augments: Vec<String>,
    pub core_items_built: u32,
    pub core_items_total: u32,
    pub missing_core_items: Vec<String>,
    pub summary: Vec<String>,
}

// ---------------------------------------------------------------------------
// P0: Data/Model sync & OCR maintenance
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetSyncStatus {
    pub patch: String,
    pub report_date: String,
    pub data_dir: String,
    pub model_dir: String,
    pub expected_data_files: usize,
    pub present_data_files: usize,
    pub missing_data_files: Vec<String>,
    pub expected_model_files: usize,
    pub present_model_files: usize,
    pub missing_model_files: Vec<String>,
    pub ready_for_offline: bool,
    pub ready_for_ocr: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetSyncResult {
    pub source_url: String,
    pub patch: String,
    pub report_date: String,
    pub downloaded_data_files: Vec<String>,
    pub downloaded_model_files: Vec<String>,
    pub skipped_files: usize,
    pub failed_files: Vec<String>,
    pub status: AssetSyncStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrCacheCleanupReport {
    pub removed_files: usize,
    pub freed_bytes: u64,
    pub scanned_dirs: Vec<String>,
}

// ---------------------------------------------------------------------------
// P1: Player markers, encounters, SGP/LCU player query & 10-player intel
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerMarker {
    pub puuid: String,
    pub summoner_name: String,
    pub tag: String,
    pub tag_label: String,
    pub note: String,
    #[serde(default)]
    pub encounters: u32,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerEncounterRecord {
    pub id: String,
    pub puuid: String,
    pub summoner_name: String,
    pub champion_name: String,
    /// `ally` or `enemy`.
    pub relation: String,
    /// `WIN`, `LOSS`, or `UNKNOWN`.
    pub result: String,
    pub kda: String,
    pub timestamp: u64,
    pub time_label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerEncounterSummary {
    pub query: String,
    pub total_encounters: usize,
    pub ally_games: usize,
    pub enemy_games: usize,
    pub ally_wins: usize,
    pub enemy_wins: usize,
    pub marker: Option<PlayerMarker>,
    pub recent: Vec<PlayerEncounterRecord>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerLibraryBundle {
    pub schema_version: String,
    pub exported_at: u64,
    #[serde(default)]
    pub markers: Vec<PlayerMarker>,
    #[serde(default)]
    pub encounters: Vec<PlayerEncounterRecord>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSearchResult {
    pub puuid: String,
    pub summoner_id: Option<u64>,
    pub summoner_name: String,
    pub tag_line: String,
    pub summoner_level: u32,
    pub profile_icon_id: u32,
    pub source: String,
    pub marker: Option<PlayerMarker>,
    pub encounter_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerProfile {
    pub summoner: PlayerSearchResult,
    pub source: String,
    pub games_analyzed: usize,
    pub win_rate: f64,
    pub avg_kda: String,
    pub avg_damage: u64,
    pub avg_hex_score: f64,
    pub top_champions: Vec<String>,
    pub recent_matches: Vec<MatchHistoryEntry>,
    pub encounter_summary: PlayerEncounterSummary,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GamePlayerIntel {
    pub puuid: String,
    pub summoner_name: String,
    pub champion_name: String,
    pub team: String,
    pub recent_games: usize,
    pub recent_win_rate: Option<f64>,
    pub avg_kda: Option<String>,
    pub avg_hex_score: Option<f64>,
    pub tags: Vec<String>,
    pub marker: Option<PlayerMarker>,
    pub encounter_count: usize,
    pub source: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntelCacheStatus {
    pub cached_players: usize,
    pub sgp_enabled: bool,
    pub last_refresh_epoch: Option<u64>,
    pub ttl_secs: u64,
}

// ---------------------------------------------------------------------------
// P1: Summoner spells, Item Set sync, ARURF & Rune Page
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummonerSpellSnapshot {
    pub in_champ_select: bool,
    pub champion_id: Option<u32>,
    pub hero_name: Option<String>,
    pub spell1_id: Option<u32>,
    pub spell2_id: Option<u32>,
    pub spell1_name: String,
    pub spell2_name: String,
    pub recommended_spell1_id: u32,
    pub recommended_spell2_id: u32,
    pub recommended_spell1_name: String,
    pub recommended_spell2_name: String,
    pub flash_slot: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemSetSyncResult {
    pub hero_name: String,
    pub champion_id: u32,
    pub title: String,
    pub item_count: usize,
    pub synced_to_client: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunePagePlan {
    pub name: String,
    pub primary_style_id: u32,
    pub sub_style_id: u32,
    pub selected_perk_ids: Vec<u32>,
    pub primary_name: String,
    pub sub_name: String,
    pub keystone_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArurfRecommendation {
    pub hero_name: String,
    pub hero_id: String,
    pub role_tag: String,
    pub spell1_id: u32,
    pub spell2_id: u32,
    pub spell_names: Vec<String>,
    pub rune_page: RunePagePlan,
    pub core_items: Vec<FormulaItemRef>,
    pub tips: Vec<String>,
}

// ---------------------------------------------------------------------------
// P1: Augment offer stats & Bench grab state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AugmentOfferStatEntry {
    pub augment_name: String,
    pub offered: u64,
    pub picked: u64,
    pub pick_rate: f64,
    pub last_seen_epoch: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AugmentOfferStatsReport {
    pub total_offers: u64,
    pub total_picks: u64,
    pub unique_augments: usize,
    pub top_offered: Vec<AugmentOfferStatEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchGrabStatus {
    pub active: bool,
    pub target_champion_id: Option<u32>,
    pub target_hero_name: Option<String>,
    pub status_text: String,
}
