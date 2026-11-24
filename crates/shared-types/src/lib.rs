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
    pub overlay_opacity: u8,
    pub overlay_zoom: u16,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LcuStatus {
    pub connected: bool,
    pub phase: String,
    pub port: Option<u16>,
    pub summoner_name: Option<String>,
    pub lockfile_path: Option<String>,
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
