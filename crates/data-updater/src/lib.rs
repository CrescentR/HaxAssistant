use std::fs;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestReport {
    pub version: String,
    pub report_date: String,
    pub expected_files: usize,
    pub present_files: usize,
    pub missing_files: Vec<String>,
    pub manifest_sha256: String,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn verify_manifest(data_dir: &Path) -> Result<ManifestReport> {
    let manifest_path = data_dir.join("manifest.json");
    let raw = fs::read(&manifest_path)?;
    let manifest_sha256 = sha256_hex(&raw);
    let json: serde_json::Value = serde_json::from_slice(&raw)?;

    let version = json
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let report_date = json
        .get("report_date")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let files: Vec<String> = json
        .get("files")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut missing_files = Vec::new();
    let mut present_files = 0usize;
    for f in &files {
        if data_dir.join(f).exists() {
            present_files += 1;
        } else {
            missing_files.push(f.clone());
        }
    }

    Ok(ManifestReport {
        version,
        report_date,
        expected_files: files.len(),
        present_files,
        missing_files,
        manifest_sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn verifies_workspace_data_manifest() {
        let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .unwrap()
            .join("data");
        let report = verify_manifest(&data_dir).expect("verify manifest");
        assert!(report.expected_files > 0);
        assert_eq!(report.present_files, report.expected_files);
        assert!(report.missing_files.is_empty());
    }
}
