use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::de::DeserializeOwned;
use serde_json::Value;
use shared_types::{
    AugmentCatalogEntry, AugmentMechanicPairs, AugmentTrio, ChampionSummary, FormulaItemBuild,
    HeroAugmentStatRow, HeroBenchmark, ItemPrice,
};

#[derive(Debug, Clone)]
pub struct Dataset {
    pub data_dir: PathBuf,
    pub runtime_meta: Value,
    pub manifest: Value,
    pub champions: Vec<ChampionSummary>,
    pub augments_full: Vec<AugmentCatalogEntry>,
    pub hero_augment_stats: HashMap<String, Vec<HeroAugmentStatRow>>,
    pub formula_items_by_hero: HashMap<String, FormulaItemBuild>,
    pub mechanic_pairs: AugmentMechanicPairs,
    pub stage_shifts: HashMap<String, HashMap<u8, f64>>,
    pub postmatch_by_hero_id: HashMap<String, Value>,
    pub hero_synergies_counters: HashMap<String, Value>,
    /// Item id -> price and recipe (`item_prices_v1.json`).
    pub item_prices: HashMap<String, ItemPrice>,
    /// Quantile levels shared by every entry of `hero_benchmarks`.
    pub benchmark_quantiles: Vec<f64>,
    /// Hero id -> per-minute benchmark distribution (`hero_postgame_benchmarks_v1.json`).
    pub hero_benchmarks: HashMap<String, HeroBenchmark>,
    /// Hero name -> strong three-augment combinations (`hero_augment_trios.json`).
    pub hero_augment_trios: HashMap<String, Vec<AugmentTrio>>,
    pub item_names: HashMap<String, String>,
}

fn read_json_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<T>(&text).unwrap_or_default(),
        Err(_) => T::default(),
    }
}

fn read_value_or_null(path: &Path) -> Value {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<Value>(&text).unwrap_or(Value::Null),
        Err(_) => Value::Null,
    }
}

impl Dataset {
    /// Resolve `data/` relative to a workspace or bundle root and load all JSON datasets into memory.
    pub fn load_from_root(root: &Path) -> Result<Self> {
        let data_dir = resolve_data_dir(root);

        let runtime_meta = read_value_or_null(&data_dir.join("runtime_meta.json"));
        let manifest = read_value_or_null(&data_dir.join("manifest.json"));
        let champions: Vec<ChampionSummary> = read_json_or_default(&data_dir.join("champions.json"));

        let augments_full: Vec<AugmentCatalogEntry> =
            if data_dir.join("augments_full.json").exists() {
                read_json_or_default(&data_dir.join("augments_full.json"))
            } else {
                read_json_or_default(&data_dir.join("augments.json"))
            };

        let hero_augment_stats: HashMap<String, Vec<HeroAugmentStatRow>> =
            read_json_or_default(&data_dir.join("hero_augment_stats.json"));

        let formula_doc = read_value_or_null(&data_dir.join("hero_formula_items.json"));
        let mut formula_items_by_hero: HashMap<String, FormulaItemBuild> = HashMap::new();
        if let Some(items) = formula_doc.get("items").and_then(|v| v.as_array()) {
            for item_val in items {
                if let Ok(build) = serde_json::from_value::<FormulaItemBuild>(item_val.clone()) {
                    formula_items_by_hero.insert(build.hero_name.clone(), build.clone());
                    formula_items_by_hero.insert(build.hero_id.clone(), build);
                }
            }
        }

        let mechanic_pairs: AugmentMechanicPairs =
            read_json_or_default(&data_dir.join("augment_mechanic_pairs_v1.json"));

        let stage_doc = read_value_or_null(&data_dir.join("augment_stage_value_v1.json"));
        let mut stage_shifts: HashMap<String, HashMap<u8, f64>> = HashMap::new();
        if let Some(rows) = stage_doc.get("rows").and_then(|v| v.as_array()) {
            for row in rows {
                let Some(name) = row.get("name").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(stage) = row.get("stage").and_then(|v| v.as_u64()) else {
                    continue;
                };
                let shift = row.get("shift").and_then(|v| v.as_f64()).unwrap_or(0.0);
                stage_shifts
                    .entry(name.to_string())
                    .or_default()
                    .insert(stage as u8, shift);
            }
        }

        let postmatch_by_hero_id: HashMap<String, Value> =
            read_json_or_default(&data_dir.join("postmatch.json"));
        let hero_synergies_counters: HashMap<String, Value> =
            read_json_or_default(&data_dir.join("hero_synergies_counters.json"));
        let item_names: HashMap<String, String> =
            read_json_or_default(&data_dir.join("item_names.json"));

        let prices_doc = read_value_or_null(&data_dir.join("item_prices_v1.json"));
        let item_prices: HashMap<String, ItemPrice> = prices_doc
            .get("items")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        let bench_doc = read_value_or_null(&data_dir.join("hero_postgame_benchmarks_v1.json"));
        let benchmark_quantiles: Vec<f64> = bench_doc
            .get("method")
            .and_then(|m| m.get("quantiles"))
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        let hero_benchmarks: HashMap<String, HeroBenchmark> = bench_doc
            .get("heroes")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        let hero_augment_trios: HashMap<String, Vec<AugmentTrio>> =
            read_json_or_default(&data_dir.join("hero_augment_trios.json"));

        Ok(Self {
            data_dir,
            item_prices,
            benchmark_quantiles,
            hero_benchmarks,
            hero_augment_trios,
            runtime_meta,
            manifest,
            champions,
            augments_full,
            hero_augment_stats,
            formula_items_by_hero,
            mechanic_pairs,
            stage_shifts,
            postmatch_by_hero_id,
            hero_synergies_counters,
            item_names,
        })
    }

    pub fn patch(&self) -> &str {
        self.runtime_meta
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("16.22")
    }

    pub fn report_date(&self) -> &str {
        self.runtime_meta
            .get("report_date")
            .and_then(|v| v.as_str())
            .unwrap_or("2026-11-09")
    }

    pub fn find_champion(&self, name_or_id: &str) -> Option<&ChampionSummary> {
        let needle = name_or_id.trim();
        self.champions.iter().find(|c| {
            c.hero == needle
                || c.id == needle
                || c.full_name.as_deref() == Some(needle)
                || c.search_terms
                    .as_ref()
                    .map(|terms| terms.iter().any(|t| t == needle))
                    .unwrap_or(false)
        })
    }
}

pub fn resolve_data_dir(root: &Path) -> PathBuf {
    let candidates = [
        root.join("data"),
        root.join("resources").join("data"),
        root.to_path_buf(),
    ];
    for cand in candidates {
        if cand.join("champions.json").exists() || cand.join("runtime_meta.json").exists() {
            return cand;
        }
    }
    root.join("data")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_workspace_dataset() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .unwrap()
            .to_path_buf();
        let ds = Dataset::load_from_root(&workspace_root).expect("load dataset");
        if workspace_root.join("data/champions.json").exists() {
            assert!(!ds.champions.is_empty(), "champions should be loaded");
            assert!(!ds.augments_full.is_empty(), "augments should be loaded");
            assert!(!ds.mechanic_pairs.links.is_empty(), "mechanic links should be loaded");
        }
    }
}
