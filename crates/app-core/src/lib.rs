use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Result};
use data_runtime::Dataset;
use data_updater::{verify_manifest, ManifestReport};
use lcu_client::LcuClient;
use serde_json::Value;
use shared_types::{
    AppSettings, AugmentRecommendationCard, EnemyCounterAnalysis, LcuStatus, LineupEvaluation,
    RuntimeSummary,
};

pub struct HexBoxCore {
    pub root: PathBuf,
    pub dataset: Dataset,
    pub lcu: LcuClient,
    pub settings: AppSettings,
}

impl HexBoxCore {
    pub fn initialize(root: &Path) -> Result<Self> {
        let dataset = Dataset::load_from_root(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            dataset,
            lcu: LcuClient::new(),
            settings: AppSettings::default(),
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

    pub fn resolve_ocr_python(&self) -> PathBuf {
        let candidates = [
            self.root.join("ocr-sidecar/.venv/bin/python"),
            self.root.join(".venv/bin/python"),
            self.root.join("ocr-sidecar/.venv/Scripts/python.exe"),
        ];
        for cand in candidates {
            if cand.exists() {
                return cand;
            }
        }
        PathBuf::from("python3")
    }

    pub fn run_ocr_preset(
        &self,
        preset_key: &str,
        image_path: Option<&str>,
    ) -> Result<Value> {
        let script = self.root.join("ocr-sidecar/hexbox_ocr.py");
        if !script.exists() {
            return Err(anyhow!("ocr script not found: {}", script.display()));
        }
        let python = self.resolve_ocr_python();
        let mut cmd = Command::new(&python);
        cmd.arg(&script)
            .arg("preset")
            .arg("--preset")
            .arg(preset_key)
            .env("ARAM_ASSISTANT_DATA_DIR", &self.dataset.data_dir)
            .env("ARAM_ASSISTANT_OCR_MODEL_DIR", self.root.join("models"));

        let resolved_img = match image_path {
            Some(p) if !p.is_empty() && Path::new(p).exists() => Some(PathBuf::from(p)),
            _ => {
                let sample = self.root.join("ocr-sidecar/sample_augment_1080p.png");
                if sample.exists() {
                    Some(sample)
                } else {
                    None
                }
            }
        };
        if let Some(img) = resolved_img {
            cmd.arg("--image").arg(img);
        }

        let output = cmd.output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("ocr sidecar exited with {}: {}", output.status, stderr));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: Value = serde_json::from_str(stdout.trim())?;
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_core_from_workspace() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .unwrap()
            .to_path_buf();
        let core = HexBoxCore::initialize(&root).expect("initialize HexBoxCore");
        let summary = core.runtime_summary();
        if root.join("data/champions.json").exists() {
            assert!(summary.hero_count >= 170);
            assert!(summary.augment_count >= 200);
            assert_eq!(summary.mechanic_link_count, 33);
            assert_eq!(summary.mechanic_conflict_count, 4);
        }
    }
}
