use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shared_types::{AssetSyncResult, AssetSyncStatus};

/// Required model artifacts for full Chinese + neural card detection OCR.
pub const REQUIRED_MODEL_FILES: &[&str] = &[
    "ch_PP-OCRv4_rec_int8.onnx",
    "ppocr_keys_v1.txt",
    "own-card-detector.onnx",
    "own-card-detector.json",
];

/// Optional extra language/v5 model artifacts fetched when present remotely.
pub const OPTIONAL_MODEL_FILES: &[&str] = &[
    "PP-OCRv5_mobile_rec.onnx",
    "PP-OCRv5_mobile_rec_keys.txt",
    "korean_PP-OCRv5_mobile_rec.onnx",
    "korean_PP-OCRv5_mobile_rec_keys.txt",
];

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

    let files = extract_manifest_files(&json);

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

fn extract_manifest_files(json: &serde_json::Value) -> Vec<String> {
    json.get("files")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn extract_manifest_hashes(json: &serde_json::Value) -> HashMap<String, String> {
    json.get("hashes")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_lowercase())))
                .collect()
        })
        .unwrap_or_default()
}

fn extract_manifest_models(json: &serde_json::Value) -> Vec<String> {
    let listed: Vec<String> = json
        .get("models")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default();
    if !listed.is_empty() {
        return listed;
    }
    REQUIRED_MODEL_FILES
        .iter()
        .chain(OPTIONAL_MODEL_FILES.iter())
        .map(|s| s.to_string())
        .collect()
}

/// Inspect local `data/` and `models/` readiness for offline recommendations and OCR.
pub fn inspect_asset_status(data_dir: &Path, model_dir: &Path) -> AssetSyncStatus {
    let report = verify_manifest(data_dir).ok();
    let (patch, report_date, expected_data_files, present_data_files, missing_data_files) =
        match report {
            Some(r) => (
                r.version,
                r.report_date,
                r.expected_files,
                r.present_files,
                r.missing_files,
            ),
            None => (
                "missing".to_string(),
                "unknown".to_string(),
                1,
                0,
                vec!["manifest.json".to_string()],
            ),
        };

    let mut missing_model_files = Vec::new();
    let mut present_model_files = 0usize;
    for name in REQUIRED_MODEL_FILES {
        if model_dir.join(name).is_file() {
            present_model_files += 1;
        } else {
            missing_model_files.push((*name).to_string());
        }
    }

    let ready_for_offline =
        expected_data_files > 0 && missing_data_files.is_empty() && data_dir.join("champions.json").is_file();
    let ready_for_ocr = missing_model_files.is_empty();

    AssetSyncStatus {
        patch,
        report_date,
        data_dir: data_dir.display().to_string(),
        model_dir: model_dir.display().to_string(),
        expected_data_files,
        present_data_files,
        missing_data_files,
        expected_model_files: REQUIRED_MODEL_FILES.len(),
        present_model_files,
        missing_model_files,
        ready_for_offline,
        ready_for_ocr,
    }
}

/// True only for plain file names: no separators, no `..`, no drive letters.
/// Anything else could make `dir.join(name)` escape the target directory
/// (e.g. absolute paths, `C:\…`, `../x`).
fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains("..")
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':')
        && Path::new(name).file_name().and_then(|n| n.to_str()) == Some(name)
}

fn write_atomic(dest: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("download.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, dest)?;
    Ok(())
}

fn file_matches_hash(path: &Path, expected_hash: Option<&str>) -> bool {
    if !path.is_file() {
        return false;
    }
    let Some(expected) = expected_hash else {
        return fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false);
    };
    fs::read(path)
        .map(|bytes| sha256_hex(&bytes).eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

async fn fetch_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let resp = client.get(url).send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("HTTP {status} for {url}"));
    }
    Ok(resp.bytes().await?.to_vec())
}

/// Synchronize data JSONs and ONNX models from a remote static mirror / release URL.
///
/// Accepts either:
/// - `<base_url>/manifest.json` (flat layout, files at `<base_url>/<file>` and models at `<base_url>/models/<file>`)
/// - `<base_url>/data/manifest.json` (repo/release layout, files at `<base_url>/data/<file>` and models at `<base_url>/models/<file>`)
pub async fn sync_remote_assets(
    base_url: &str,
    data_dir: &Path,
    model_dir: &Path,
    include_models: bool,
) -> Result<AssetSyncResult> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(anyhow!(
            "未配置远程数据同步地址，请先在设置中填写数据源 URL（例如 GitHub Release 或静态镜像根路径）"
        ));
    }

    fs::create_dir_all(data_dir)?;
    fs::create_dir_all(model_dir)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;

    let (manifest_bytes, data_base, model_base) =
        match fetch_bytes(&client, &format!("{trimmed}/data/manifest.json")).await {
            Ok(bytes) => (bytes, format!("{trimmed}/data"), format!("{trimmed}/models")),
            Err(_) => {
                let bytes = fetch_bytes(&client, &format!("{trimmed}/manifest.json")).await?;
                (bytes, trimmed.to_string(), format!("{trimmed}/models"))
            }
        };

    let manifest_json: serde_json::Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| anyhow!("远程 manifest.json 解析失败: {e}"))?;
    let patch = manifest_json
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let report_date = manifest_json
        .get("report_date")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let files = extract_manifest_files(&manifest_json);
    let hashes = extract_manifest_hashes(&manifest_json);

    let mut downloaded_data_files = Vec::new();
    let mut downloaded_model_files = Vec::new();
    let mut failed_files = Vec::new();
    let mut skipped_files = 0usize;

    for file_name in &files {
        if !is_safe_file_name(file_name) {
            continue;
        }
        let dest = data_dir.join(file_name);
        let expected_hash = hashes.get(file_name).map(String::as_str);
        if file_matches_hash(&dest, expected_hash) {
            skipped_files += 1;
            continue;
        }
        let url = format!("{data_base}/{file_name}");
        match fetch_bytes(&client, &url).await {
            Ok(bytes) => {
                if let Some(exp) = expected_hash {
                    let got = sha256_hex(&bytes);
                    if !got.eq_ignore_ascii_case(exp) {
                        failed_files.push(format!("{file_name} (sha256 mismatch)"));
                        continue;
                    }
                }
                write_atomic(&dest, &bytes)?;
                downloaded_data_files.push(file_name.clone());
            }
            Err(err) => failed_files.push(format!("{file_name}: {err}")),
        }
    }

    write_atomic(&data_dir.join("manifest.json"), &manifest_bytes)?;

    if include_models {
        let model_hashes: HashMap<String, String> = manifest_json
            .get("model_hashes")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_lowercase())))
                    .collect()
            })
            .unwrap_or_default();

        for model_name in extract_manifest_models(&manifest_json) {
            if !is_safe_file_name(&model_name) {
                continue;
            }
            let dest = model_dir.join(&model_name);
            let expected_hash = model_hashes.get(&model_name).map(String::as_str);
            if file_matches_hash(&dest, expected_hash) {
                skipped_files += 1;
                continue;
            }
            let is_required = REQUIRED_MODEL_FILES.contains(&model_name.as_str());
            let url = format!("{model_base}/{model_name}");
            match fetch_bytes(&client, &url).await {
                Ok(bytes) => {
                    if let Some(exp) = expected_hash {
                        let got = sha256_hex(&bytes);
                        if !got.eq_ignore_ascii_case(exp) {
                            failed_files.push(format!("models/{model_name} (sha256 mismatch)"));
                            continue;
                        }
                    }
                    write_atomic(&dest, &bytes)?;
                    downloaded_model_files.push(model_name);
                }
                Err(err) => {
                    if is_required {
                        failed_files.push(format!("models/{model_name}: {err}"));
                    }
                }
            }
        }
    }

    let status = inspect_asset_status(data_dir, model_dir);
    Ok(AssetSyncResult {
        source_url: trimmed.to_string(),
        patch,
        report_date,
        downloaded_data_files,
        downloaded_model_files,
        skipped_files,
        failed_files,
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn rejects_unsafe_manifest_file_names() {
        assert!(is_safe_file_name("champions.json"));
        assert!(is_safe_file_name("ch_PP-OCRv4_rec_int8.onnx"));
        assert!(!is_safe_file_name("../evil.json"));
        assert!(!is_safe_file_name("..\\evil.json"));
        assert!(!is_safe_file_name("/etc/passwd"));
        assert!(!is_safe_file_name("\\server\\share"));
        assert!(!is_safe_file_name("C:\\Windows\\x.dll"));
        assert!(!is_safe_file_name("sub/dir.json"));
        assert!(!is_safe_file_name("sub\\dir.json"));
        assert!(!is_safe_file_name(""));
    }

    #[test]
    fn verifies_workspace_data_manifest() {
        let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .unwrap()
            .join("data");
        if !data_dir.join("manifest.json").exists() {
            // `/data/` is gitignored in CI; verify with a synthetic temp manifest instead.
            let tmp_dir = std::env::temp_dir().join("hexbox_manifest_test");
            let _ = fs::create_dir_all(&tmp_dir);
            fs::write(tmp_dir.join("sample.json"), b"{}").expect("write sample");
            fs::write(
                tmp_dir.join("manifest.json"),
                br#"{"version":"16.22","report_date":"2026-11-09","files":["sample.json"]}"#,
            )
            .expect("write manifest");
            let report = verify_manifest(&tmp_dir).expect("verify synthetic manifest");
            assert_eq!(report.expected_files, 1);
            assert_eq!(report.present_files, 1);
            assert!(report.missing_files.is_empty());
            let _ = fs::remove_dir_all(&tmp_dir);
            return;
        }
        let report = verify_manifest(&data_dir).expect("verify manifest");
        assert!(report.expected_files > 0);
        assert_eq!(report.present_files, report.expected_files);
        assert!(report.missing_files.is_empty());
    }

    #[tokio::test]
    async fn syncs_remote_manifest_data_and_models_with_sha256_check() {
        let champs_bytes = r#"[{"id":"22","hero":"寒冰射手"}]"#.as_bytes().to_vec();
        let champs_hash = sha256_hex(&champs_bytes);
        let model_bytes = b"fake-onnx-model".to_vec();
        let model_hash = sha256_hex(&model_bytes);
        let manifest_body = serde_json::json!({
            "version": "16.24",
            "report_date": "2026-11-25",
            "files": ["champions.json"],
            "hashes": { "champions.json": champs_hash },
            "models": ["ch_PP-OCRv4_rec_int8.onnx"],
            "model_hashes": { "ch_PP-OCRv4_rec_int8.onnx": model_hash }
        })
        .to_string()
        .into_bytes();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let routes: HashMap<String, Vec<u8>> = HashMap::from([
            ("/data/manifest.json".to_string(), manifest_body),
            ("/data/champions.json".to_string(), champs_bytes),
            ("/models/ch_PP-OCRv4_rec_int8.onnx".to_string(), model_bytes),
        ]);

        let server = tokio::spawn(async move {
            for _ in 0..8 {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0u8; 2048];
                let n = socket.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/");
                if let Some(body) = routes.get(path) {
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(header.as_bytes()).await;
                    let _ = socket.write_all(body).await;
                } else {
                    let _ = socket
                        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                }
            }
        });

        let root = std::env::temp_dir().join(format!(
            "hexbox_sync_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let data_dir = root.join("data");
        let model_dir = root.join("models");

        let res = sync_remote_assets(&format!("http://{addr}"), &data_dir, &model_dir, true)
            .await
            .expect("sync remote assets");
        assert_eq!(res.patch, "16.24");
        assert_eq!(res.downloaded_data_files, vec!["champions.json"]);
        assert_eq!(res.downloaded_model_files, vec!["ch_PP-OCRv4_rec_int8.onnx"]);
        assert!(res.failed_files.is_empty());
        assert!(data_dir.join("champions.json").exists());
        assert!(model_dir.join("ch_PP-OCRv4_rec_int8.onnx").exists());

        // Second sync should skip unchanged files via hash check.
        let res2 = sync_remote_assets(&format!("http://{addr}"), &data_dir, &model_dir, true)
            .await
            .expect("second sync skips");
        assert!(res2.downloaded_data_files.is_empty());
        assert!(res2.downloaded_model_files.is_empty());
        assert_eq!(res2.skipped_files, 2);

        server.abort();
        let _ = fs::remove_dir_all(&root);
    }
}
