//! Whole-bundle data updates, modelled on HexBox 1.1.0's `data-updater`.
//!
//! 1. Fetch a small meta document (`/api/hexdata/meta`) and compare its
//!    `corePayloadHash` with the local `manifest.json`. Equal → nothing to do.
//! 2. Otherwise download one `tar.gz` runtime bundle, check its size and
//!    SHA-256 against the meta, and unpack it into a staging directory.
//! 3. Validate the staged files (manifest hashes, core hash, required data).
//! 4. Move the bundle's files into the data directory, manifest last, keeping
//!    the replaced files in `.backup/` and restoring them if a move fails.
//!    Files that are not part of the bundle (user settings, player markers,
//!    window layouts) are never touched.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use serde::{de::DeserializeOwned, Deserialize};
use shared_types::{AugmentCatalogEntry, ChampionSummary, DataUpdateOutcome};

use crate::{
    extract_manifest_files, extract_manifest_hashes, file_matches_hash, is_safe_file_name,
    is_user_private_file, sha256_hex, write_atomic,
};

#[derive(Debug)]
pub struct MetaEndpointNotFound;

impl std::fmt::Display for MetaEndpointNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("数据版本信息 (meta) 返回 HTTP 404")
    }
}

impl std::error::Error for MetaEndpointNotFound {}

pub const META_PATH: &str = "/api/hexdata/meta";
pub const BUNDLE_PATH: &str = "/api/hexdata/runtime-bundle";

const META_LIMIT_BYTES: u64 = 256 * 1024;
const BUNDLE_LIMIT_BYTES: u64 = 64 * 1024 * 1024;
const ENTRY_LIMIT_BYTES: u64 = 48 * 1024 * 1024;
const UNPACKED_LIMIT_BYTES: u64 = 256 * 1024 * 1024;
const STAGING_PREFIX: &str = ".staging-";
const BACKUP_DIR: &str = ".backup";
const PENDING_FILE: &str = ".runtime-update-pending.json";

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteMeta {
    #[serde(default = "default_true")]
    pub data_updater_enabled: bool,
    #[serde(default)]
    pub snapshot_id: String,
    #[serde(default)]
    pub core_payload_hash: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub report_date: Option<String>,
    #[serde(default)]
    pub bundle: RemoteBundle,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RemoteBundle {
    pub sha256: String,
    pub bytes: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn local_core_payload_hash(data_dir: &Path) -> Option<String> {
    let raw = fs::read(data_dir.join("manifest.json")).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    json.get("core_payload_hash")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
}

async fn fetch_limited(
    client: &reqwest::Client,
    base: &str,
    path: &str,
    limit: u64,
    what: &str,
) -> Result<Vec<u8>> {
    let mut resp = client
        .get(format!("{base}{path}"))
        .send()
        .await
        .map_err(reqwest::Error::without_url)
        .with_context(|| format!("{what} 请求失败"))?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND && path == META_PATH {
        return Err(MetaEndpointNotFound.into());
    }
    if !status.is_success() {
        return Err(anyhow!("{what} 返回 HTTP {status}"));
    }
    if resp.content_length().is_some_and(|len| len > limit) {
        return Err(anyhow!("{what} 超过 {limit} 字节的安全上限"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(reqwest::Error::without_url)? {
        if bytes.len() as u64 + chunk.len() as u64 > limit {
            return Err(anyhow!("{what} 超过 {limit} 字节的安全上限"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub async fn fetch_remote_meta(client: &reqwest::Client, base_url: &str) -> Result<RemoteMeta> {
    let bytes = fetch_limited(
        client,
        base_url,
        META_PATH,
        META_LIMIT_BYTES,
        "数据版本信息 (meta)",
    )
    .await?;
    let mut meta: RemoteMeta =
        serde_json::from_slice(&bytes).map_err(|e| anyhow!("数据版本信息 (meta) 解析失败: {e}"))?;
    if meta.data_updater_enabled {
        meta.core_payload_hash = meta.core_payload_hash.trim().to_ascii_lowercase();
        if !valid_hash(&meta.core_payload_hash) {
            return Err(anyhow!("数据版本信息 (meta) 的 corePayloadHash 无效"));
        }
        if !valid_hash(&meta.bundle.sha256) {
            return Err(anyhow!("数据版本信息 (meta) 的数据包 sha256 无效"));
        }
    }
    Ok(meta)
}

fn is_complete_snapshot(data_dir: &Path) -> bool {
    let Ok(raw) = fs::read(data_dir.join("manifest.json")) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(&raw) else {
        return false;
    };
    let files = extract_manifest_files(&manifest);
    let hashes = extract_manifest_hashes(&manifest);
    let listed = |name: &str| files.iter().any(|file| file == name);
    let augment_file = ["augments_full.json", "augments.json"]
        .into_iter()
        .find(|file| listed(file));
    listed("champions.json")
        && validate_rows::<ChampionSummary>(&data_dir.join("champions.json")).is_ok()
        && augment_file
            .is_some_and(|name| validate_rows::<AugmentCatalogEntry>(&data_dir.join(name)).is_ok())
        && files.iter().all(|name| {
            name == "manifest.json"
                || is_user_private_file(name)
                || (is_runtime_file_name(name)
                    && hashes
                        .get(name)
                        .is_some_and(|hash| file_matches_hash(&data_dir.join(name), Some(hash))))
        })
}

fn is_runtime_file_name(name: &str) -> bool {
    !name.starts_with('.') && name.ends_with(".json") && is_safe_file_name(name)
}

/// Unpack plain, top-level files only; anything else aborts the update.
fn unpack_bundle(bytes: &[u8], staging: &Path) -> Result<()> {
    let mut archive = tar::Archive::new(GzDecoder::new(bytes));
    let mut total = 0u64;
    let mut names = HashSet::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        if entry.header().entry_type().is_dir() && path == Path::new(".") {
            continue;
        }
        if !entry.header().entry_type().is_file() {
            return Err(anyhow!("数据包包含非普通文件条目"));
        }
        let name = path.to_str().context("数据包文件名必须使用 UTF-8")?;
        let name = name.strip_prefix("./").unwrap_or(name).to_string();
        if !is_runtime_file_name(&name) || !names.insert(name.to_ascii_lowercase()) {
            return Err(anyhow!("数据包包含非法文件名: {name}"));
        }
        let size = entry.header().size()?;
        total = total.saturating_add(size);
        if size > ENTRY_LIMIT_BYTES || total > UNPACKED_LIMIT_BYTES {
            return Err(anyhow!("数据包解压后体积超过安全上限"));
        }
        let mut buf = Vec::with_capacity(size as usize);
        entry
            .by_ref()
            .take(ENTRY_LIMIT_BYTES + 1)
            .read_to_end(&mut buf)?;
        if buf.len() as u64 != size {
            return Err(anyhow!("数据包条目 {name} 长度与声明不符"));
        }
        fs::write(staging.join(&name), &buf)?;
    }
    Ok(())
}

fn validate_rows<T: DeserializeOwned>(path: &Path) -> Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let rows: Vec<T> = serde_json::from_slice(&fs::read(path)?)
        .with_context(|| format!("数据包 {name} 格式无效"))?;
    if rows.is_empty() {
        return Err(anyhow!("数据包 {name} 为空"));
    }
    Ok(())
}

/// Returns the files to install (manifest last).
fn validate_staged(staging: &Path, meta: &RemoteMeta) -> Result<Vec<String>> {
    let raw = fs::read(staging.join("manifest.json")).context("数据包缺少 manifest.json")?;
    let manifest: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|e| anyhow!("数据包 manifest.json 解析失败: {e}"))?;

    let core = manifest
        .get("core_payload_hash")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !core.eq_ignore_ascii_case(meta.core_payload_hash.trim()) {
        return Err(anyhow!("数据包 core_payload_hash 与版本信息不一致"));
    }
    if manifest
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .is_empty()
    {
        return Err(anyhow!("数据包 manifest.json 缺少 version"));
    }
    let schema = manifest.get("desktop_data_schema_version");
    if !schema.is_some_and(|value| value.as_str() == Some("2") || value.as_u64() == Some(2)) {
        return Err(anyhow!("数据包结构版本与当前桌面端（2）不兼容"));
    }

    let files = extract_manifest_files(&manifest);
    if files.is_empty() {
        return Err(anyhow!("数据包 manifest.json 未列出任何文件"));
    }
    let hashes = extract_manifest_hashes(&manifest);
    let mut install = Vec::with_capacity(files.len() + 1);
    let mut names = HashSet::new();
    for name in &files {
        if !is_runtime_file_name(name) || !names.insert(name.to_ascii_lowercase()) {
            return Err(anyhow!("数据包 manifest.json 含非法文件名: {name}"));
        }
        if name == "manifest.json" || is_user_private_file(name) {
            continue;
        }
        let expected = hashes
            .get(name)
            .ok_or_else(|| anyhow!("数据包 manifest.json 缺少 {name} 的哈希"))?;
        let bytes = fs::read(staging.join(name)).with_context(|| format!("数据包缺少 {name}"))?;
        if !sha256_hex(&bytes).eq_ignore_ascii_case(expected) {
            return Err(anyhow!("数据包文件 {name} 哈希校验失败"));
        }
        install.push(name.clone());
    }

    let listed = |file: &str| install.iter().any(|name| name == file);
    if !listed("champions.json") {
        return Err(anyhow!("数据包 manifest.json 未列出 champions.json"));
    }
    let augment_file = ["augments_full.json", "augments.json"]
        .into_iter()
        .find(|file| listed(file))
        .context("数据包 manifest.json 未列出海克斯列表")?;
    validate_rows::<ChampionSummary>(&staging.join("champions.json"))?;
    validate_rows::<AugmentCatalogEntry>(&staging.join(augment_file))?;

    install.push("manifest.json".to_string());
    Ok(install)
}

fn restore_backup(data_dir: &Path, backup: &Path, files: &[String]) -> Result<()> {
    let mut failures = Vec::new();
    for name in files.iter().rev() {
        let saved = backup.join(name);
        let target = data_dir.join(name);
        let restored = if saved.is_file() {
            fs::read(saved)
                .map_err(anyhow::Error::from)
                .and_then(|bytes| write_atomic(&target, &bytes))
        } else {
            match fs::remove_file(&target) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
                Err(err) => Err(err.into()),
            }
        };
        if let Err(err) = restored {
            failures.push(format!("{name}: {err}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("{}", failures.join("; ")))
    }
}

fn lock_data_dir(data_dir: &Path) -> Result<File> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data_dir.join(".runtime-update.lock"))?;
    lock.try_lock()
        .map_err(|err| anyhow!("无法锁定数据更新目录: {err}"))?;
    Ok(lock)
}

fn recover_pending(data_dir: &Path) -> Result<()> {
    let pending = data_dir.join(PENDING_FILE);
    let bytes = match fs::read(&pending) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    let files: Vec<String> = serde_json::from_slice(&bytes).context("数据更新恢复记录格式无效")?;
    if files
        .iter()
        .any(|name| !is_runtime_file_name(name) || is_user_private_file(name))
    {
        return Err(anyhow!("数据更新恢复记录包含非法文件名"));
    }
    restore_backup(data_dir, &data_dir.join(BACKUP_DIR), &files)?;
    fs::remove_file(pending)?;
    Ok(())
}

/// Roll back an interrupted install before loading the on-disk dataset.
pub fn recover_runtime_update(data_dir: &Path) -> Result<()> {
    if !data_dir.join(PENDING_FILE).is_file() {
        return Ok(());
    }
    let _lock = lock_data_dir(data_dir)?;
    recover_pending(data_dir)
}

fn obsolete_files(data_dir: &Path, install: &[String]) -> Vec<String> {
    let manifest: serde_json::Value = fs::read(data_dir.join("manifest.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    extract_manifest_files(&manifest)
        .into_iter()
        .filter(|name| {
            is_runtime_file_name(name)
                && !is_user_private_file(name)
                && !install.iter().any(|next| next.eq_ignore_ascii_case(name))
        })
        .collect()
}

fn commit_staged(data_dir: &Path, staging: &Path, install: &[String]) -> Result<()> {
    let obsolete = obsolete_files(data_dir, install);
    let affected: Vec<_> = install.iter().chain(&obsolete).cloned().collect();
    let backup = data_dir.join(BACKUP_DIR);
    match fs::remove_dir_all(&backup) {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => return Err(err).context("无法清理旧数据备份"),
    }
    fs::create_dir(&backup)?;
    for name in &affected {
        let current = data_dir.join(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_file() => {
                fs::copy(&current, backup.join(name))
                    .with_context(|| format!("备份 {name} 失败"))?;
            }
            Ok(_) => return Err(anyhow!("现有数据 {name} 不是普通文件")),
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => return Err(err).with_context(|| format!("无法读取现有数据 {name}")),
        }
    }
    let pending = data_dir.join(PENDING_FILE);
    write_atomic(&pending, &serde_json::to_vec(&affected)?)?;
    let commit = (|| -> Result<()> {
        for name in &obsolete {
            match fs::remove_file(data_dir.join(name)) {
                Ok(()) => {}
                Err(err) if err.kind() == ErrorKind::NotFound => {}
                Err(err) => return Err(err).with_context(|| format!("删除旧数据 {name} 失败")),
            }
        }
        for name in install {
            fs::rename(staging.join(name), data_dir.join(name))
                .with_context(|| format!("替换 {name} 失败"))?;
        }
        fs::remove_file(&pending).context("无法完成数据安装")?;
        Ok(())
    })();
    if let Err(err) = commit {
        return match recover_pending(data_dir) {
            Ok(()) => Err(err.context("数据安装失败，已恢复原数据")),
            Err(restore_err) => Err(anyhow!(
                "数据安装失败: {err:#}；恢复原数据失败: {restore_err:#}"
            )),
        };
    }
    Ok(())
}

fn create_staging_dir(data_dir: &Path) -> Result<PathBuf> {
    let staging = data_dir.join(format!(
        "{STAGING_PREFIX}{}-{}",
        std::process::id(),
        now_ms()
    ));
    fs::create_dir(&staging)?;
    Ok(staging)
}

/// Seed a new data directory from the installer while preserving existing snapshots.
pub fn seed_bundled_data(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    let _lock = lock_data_dir(destination)?;
    recover_pending(destination)?;
    if destination.join("champions.json").is_file()
        && (destination.join("augments_full.json").is_file()
            || destination.join("augments.json").is_file())
        && destination.join("manifest.json").is_file()
    {
        return Ok(());
    }
    if !source.join("manifest.json").is_file() || !source.join("champions.json").is_file() {
        return Err(anyhow!("应用未附带完整的初始数据"));
    }
    let staging = create_staging_dir(destination)?;
    let result = (|| {
        let mut install = Vec::new();
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type()?.is_file()
                && is_runtime_file_name(&name)
                && !is_user_private_file(&name)
            {
                fs::copy(entry.path(), staging.join(&name))?;
                install.push(name);
            }
        }
        install.sort_by_key(|name| name == "manifest.json");
        commit_staged(destination, &staging, &install)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

fn outcome(
    status: &str,
    base_url: &str,
    meta: Option<&RemoteMeta>,
    previous: Option<String>,
) -> DataUpdateOutcome {
    DataUpdateOutcome {
        status: status.to_string(),
        source_url: base_url.to_string(),
        snapshot_id: meta.map(|m| m.snapshot_id.clone()).unwrap_or_default(),
        patch: meta.and_then(|m| m.version.clone()).unwrap_or_default(),
        report_date: meta.and_then(|m| m.report_date.clone()).unwrap_or_default(),
        remote_hash: meta
            .map(|m| m.core_payload_hash.to_lowercase())
            .unwrap_or_default(),
        previous_hash: previous.unwrap_or_default(),
        updated_files: Vec::new(),
        message: String::new(),
        checked_at_ms: now_ms(),
    }
}

/// Check the remote meta and, when `apply` is true and the hash differs,
/// download, verify and install the runtime bundle.
///
/// Statuses: `disabled` (server switched updates off), `already_latest`,
/// `available` (newer data exists but `apply` was false), `updated`.
pub async fn run_runtime_update(
    base_url: &str,
    data_dir: &Path,
    apply: bool,
) -> Result<DataUpdateOutcome> {
    run_runtime_update_if(base_url, data_dir, apply, || true).await
}

pub async fn run_runtime_update_if(
    base_url: &str,
    data_dir: &Path,
    apply: bool,
    can_install: impl FnOnce() -> bool,
) -> Result<DataUpdateOutcome> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(anyhow!("未配置远程数据同步地址"));
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;

    recover_runtime_update(data_dir)?;
    let meta = fetch_remote_meta(&client, base).await?;
    let previous = local_core_payload_hash(data_dir);

    if !meta.data_updater_enabled {
        let mut out = outcome("disabled", base, Some(&meta), previous);
        out.message = "服务端已暂停数据更新".to_string();
        return Ok(out);
    }
    if previous.as_deref() == Some(meta.core_payload_hash.as_str())
        && is_complete_snapshot(data_dir)
    {
        let mut out = outcome("already_latest", base, Some(&meta), previous);
        out.message = "本地数据已是最新".to_string();
        return Ok(out);
    }
    if !apply {
        let mut out = outcome("available", base, Some(&meta), previous);
        out.message = "发现新数据，等待对局结束后更新".to_string();
        return Ok(out);
    }

    if meta.bundle.bytes == 0 || meta.bundle.bytes > BUNDLE_LIMIT_BYTES {
        return Err(anyhow!(
            "数据包声明大小 {} 字节超出允许范围",
            meta.bundle.bytes
        ));
    }
    let bytes = fetch_limited(
        &client,
        base,
        BUNDLE_PATH,
        meta.bundle.bytes,
        "运行时数据包",
    )
    .await?;
    if bytes.len() as u64 != meta.bundle.bytes {
        return Err(anyhow!("运行时数据包大小与版本信息不一致"));
    }
    if !sha256_hex(&bytes).eq_ignore_ascii_case(&meta.bundle.sha256) {
        return Err(anyhow!("运行时数据包 SHA-256 校验失败"));
    }

    fs::create_dir_all(data_dir)?;
    let _lock = lock_data_dir(data_dir)?;
    recover_pending(data_dir)?;
    if local_core_payload_hash(data_dir).as_deref() == Some(meta.core_payload_hash.as_str())
        && is_complete_snapshot(data_dir)
    {
        let mut out = outcome("already_latest", base, Some(&meta), previous);
        out.message = "本地数据已是最新".to_string();
        return Ok(out);
    }
    let staging = create_staging_dir(data_dir)?;
    let result = (|| {
        unpack_bundle(&bytes, &staging)?;
        let install = validate_staged(&staging, &meta)?;
        if !can_install() {
            return Ok::<_, anyhow::Error>(None);
        }
        commit_staged(data_dir, &staging, &install)?;
        Ok(Some(install))
    })();
    let _ = fs::remove_dir_all(&staging);
    let Some(install) = result? else {
        let mut out = outcome("available", base, Some(&meta), previous);
        out.message = "发现新数据，等待对局结束后更新".to_string();
        return Ok(out);
    };

    let mut out = outcome("updated", base, Some(&meta), previous);
    out.message = format!(
        "已更新到 {}",
        if out.patch.is_empty() {
            &out.snapshot_id
        } else {
            &out.patch
        }
    );
    out.updated_files = install;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn build_bundle(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(enc);
        for (name, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, name, bytes.as_slice())
                .unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn payload(core: &str, champs: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let augs = br#"[{"augment_id":1,"augment_name":"Test augment"}]"#.to_vec();
        let manifest = serde_json::json!({
            "version": "16.23",
            "report_date": "2026-11-26",
            "desktop_data_schema_version": "2",
            "core_payload_hash": core,
            "files": ["champions.json", "augments_full.json"],
            "hashes": { "champions.json": sha256_hex(champs), "augments_full.json": sha256_hex(&augs) },
        })
        .to_string()
        .into_bytes();
        let bundle = build_bundle(&[
            ("manifest.json", manifest.clone()),
            ("champions.json", champs.to_vec()),
            ("augments_full.json", augs),
        ]);
        (bundle, manifest)
    }

    type Routes = Arc<Mutex<HashMap<String, Vec<u8>>>>;

    async fn serve(routes: Routes) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0u8; 2048];
                let n = socket.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .to_string();
                let body = routes.lock().unwrap().get(&path).cloned();
                match body {
                    Some(body) => {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = socket.write_all(header.as_bytes()).await;
                        let _ = socket.write_all(&body).await;
                    }
                    None => {
                        let _ = socket.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                    }
                }
            }
        });
        (format!("http://{addr}"), handle)
    }

    fn meta_json(core: &str, bundle: &[u8], enabled: bool) -> Vec<u8> {
        serde_json::json!({
            "dataUpdaterEnabled": enabled,
            "snapshotId": "aramhex/16.23/normal/seq-22",
            "corePayloadHash": core,
            "version": "16.23",
            "reportDate": "2026-11-26",
            "bundle": { "sha256": sha256_hex(bundle), "bytes": bundle.len() },
        })
        .to_string()
        .into_bytes()
    }

    fn temp_data_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hexbox_bundle_{tag}_{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn applies_bundle_keeps_user_files_and_skips_when_current() {
        let core = "a".repeat(64);
        let champs = r#"[{"id":"22","hero":"寒冰射手"}]"#.as_bytes().to_vec();
        let (bundle, _) = payload(&core, &champs);
        let routes: Routes = Arc::new(Mutex::new(HashMap::from([
            (META_PATH.to_string(), meta_json(&core, &bundle, true)),
            (BUNDLE_PATH.to_string(), bundle),
        ])));
        let (base, server) = serve(routes.clone()).await;

        let data_dir = temp_data_dir("apply");
        fs::write(data_dir.join("champions.json"), b"[\"old\"]").unwrap();
        fs::write(
            data_dir.join("manifest.json"),
            br#"{"core_payload_hash":"old"}"#,
        )
        .unwrap();
        fs::write(data_dir.join("user_settings.json"), b"{\"keep\":true}").unwrap();

        let checked = run_runtime_update(&base, &data_dir, false).await.unwrap();
        assert_eq!(checked.status, "available");
        assert_eq!(
            fs::read(data_dir.join("champions.json")).unwrap(),
            b"[\"old\"]"
        );

        let out = run_runtime_update(&base, &data_dir, true).await.unwrap();
        assert_eq!(out.status, "updated");
        assert_eq!(
            out.updated_files.last().map(String::as_str),
            Some("manifest.json")
        );
        assert_eq!(fs::read(data_dir.join("champions.json")).unwrap(), champs);
        assert_eq!(
            local_core_payload_hash(&data_dir).as_deref(),
            Some(core.as_str())
        );
        assert_eq!(
            fs::read(data_dir.join("user_settings.json")).unwrap(),
            b"{\"keep\":true}"
        );
        assert_eq!(
            fs::read(data_dir.join(BACKUP_DIR).join("champions.json")).unwrap(),
            b"[\"old\"]"
        );
        assert!(!fs::read_dir(&data_dir)
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with(STAGING_PREFIX)));

        let again = run_runtime_update(&base, &data_dir, true).await.unwrap();
        assert_eq!(again.status, "already_latest");

        fs::remove_file(data_dir.join("champions.json")).unwrap();
        let deferred = run_runtime_update_if(&base, &data_dir, true, || false)
            .await
            .unwrap();
        assert_eq!(deferred.status, "available");
        assert!(!data_dir.join("champions.json").exists());

        let repaired = run_runtime_update(&base, &data_dir, true).await.unwrap();
        assert_eq!(repaired.status, "updated");
        assert_eq!(fs::read(data_dir.join("champions.json")).unwrap(), champs);

        routes.lock().unwrap().insert(
            META_PATH.to_string(),
            meta_json(&"b".repeat(64), b"x", false),
        );
        let off = run_runtime_update(&base, &data_dir, true).await.unwrap();
        assert_eq!(off.status, "disabled");

        server.abort();
        let _ = fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn recovers_interrupted_install_and_preserves_user_state() {
        let data_dir = temp_data_dir("recover");
        let backup = data_dir.join(BACKUP_DIR);
        fs::create_dir(&backup).unwrap();
        fs::write(backup.join("champions.json"), b"[1]").unwrap();
        fs::write(data_dir.join("champions.json"), b"[2]").unwrap();
        fs::write(data_dir.join("new.json"), b"[]").unwrap();
        fs::write(data_dir.join("user_settings.json"), b"{}").unwrap();
        fs::write(
            data_dir.join(PENDING_FILE),
            br#"["champions.json","new.json"]"#,
        )
        .unwrap();

        recover_runtime_update(&data_dir).unwrap();
        recover_runtime_update(&data_dir).unwrap();
        assert_eq!(fs::read(data_dir.join("champions.json")).unwrap(), b"[1]");
        assert!(!data_dir.join("new.json").exists());
        assert!(!data_dir.join(PENDING_FILE).exists());
        assert_eq!(
            fs::read(data_dir.join("user_settings.json")).unwrap(),
            b"{}"
        );
        fs::remove_dir_all(&data_dir).unwrap();
    }

    #[test]
    fn seed_is_idempotent_after_a_synced_snapshot() {
        let bundled = temp_data_dir("seed_source");
        let local = temp_data_dir("seed_destination");
        fs::write(bundled.join("manifest.json"), b"{}").unwrap();
        fs::write(bundled.join("champions.json"), b"[1]").unwrap();
        fs::write(bundled.join("augments_full.json"), b"[1]").unwrap();
        fs::write(bundled.join("USER_SETTINGS.json"), b"{\"bundled\":true}").unwrap();
        seed_bundled_data(&bundled, &local).unwrap();
        assert!(!local.join("USER_SETTINGS.json").exists());
        fs::write(local.join("champions.json"), b"[2]").unwrap();
        seed_bundled_data(&bundled, &local).unwrap();
        assert_eq!(fs::read(local.join("champions.json")).unwrap(), b"[2]");
        fs::remove_dir_all(&local).unwrap();
        fs::remove_dir_all(&bundled).unwrap();
    }

    #[tokio::test]
    async fn rejects_tampered_bundle_and_keeps_old_data() {
        let core = "c".repeat(64);
        let (bundle, _) = payload(&core, br#"[{"id":"1","hero":"Test champion"}]"#);
        let mut tampered = bundle.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xff;
        let routes: Routes = Arc::new(Mutex::new(HashMap::from([
            (META_PATH.to_string(), meta_json(&core, &bundle, true)),
            (BUNDLE_PATH.to_string(), tampered),
        ])));
        let (base, server) = serve(routes.clone()).await;
        let data_dir = temp_data_dir("tamper");
        fs::write(data_dir.join("champions.json"), b"[\"old\"]").unwrap();

        let err = run_runtime_update(&base, &data_dir, true)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("SHA-256"), "{err}");
        assert_eq!(
            fs::read(data_dir.join("champions.json")).unwrap(),
            b"[\"old\"]"
        );

        // Bundle hash fine, but its manifest disagrees with the meta's core hash.
        let (other, _) = payload(&"d".repeat(64), br#"[{"id":"1","hero":"Test champion"}]"#);
        routes
            .lock()
            .unwrap()
            .insert(META_PATH.to_string(), meta_json(&core, &other, true));
        routes
            .lock()
            .unwrap()
            .insert(BUNDLE_PATH.to_string(), other);
        let err = run_runtime_update(&base, &data_dir, true)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("core_payload_hash"), "{err}");
        assert_eq!(
            fs::read(data_dir.join("champions.json")).unwrap(),
            b"[\"old\"]"
        );

        server.abort();
        let _ = fs::remove_dir_all(&data_dir);
    }

    #[tokio::test]
    async fn never_overwrites_user_private_files_even_if_listed_in_bundle() {
        let core = "e".repeat(64);
        let champs = r#"[{"id":"22","hero":"寒冰射手"}]"#.as_bytes().to_vec();
        let augs = br#"[{"augment_id":1,"augment_name":"Test augment"}]"#.to_vec();
        let remote_stats = br#"{"total_offers":999}"#.to_vec();
        let manifest = serde_json::json!({
            "version": "16.24",
            "report_date": "2026-11-27",
            "desktop_data_schema_version": "2",
            "core_payload_hash": core,
            "files": ["champions.json", "augments_full.json", "augment_offer_stats.json"],
            "hashes": {
                "champions.json": sha256_hex(&champs),
                "augments_full.json": sha256_hex(&augs),
                "augment_offer_stats.json": sha256_hex(&remote_stats)
            }
        })
        .to_string()
        .into_bytes();
        let bundle = build_bundle(&[
            ("manifest.json", manifest),
            ("champions.json", champs.clone()),
            ("augments_full.json", augs),
            ("augment_offer_stats.json", remote_stats),
        ]);
        let routes: Routes = Arc::new(Mutex::new(HashMap::from([
            (META_PATH.to_string(), meta_json(&core, &bundle, true)),
            (BUNDLE_PATH.to_string(), bundle),
        ])));
        let (base, server) = serve(routes).await;

        let data_dir = temp_data_dir("private_guard");
        fs::write(data_dir.join("augment_offer_stats.json"), b"{\"local\":1}").unwrap();

        let out = run_runtime_update(&base, &data_dir, true).await.unwrap();
        assert_eq!(out.status, "updated");
        assert!(!out
            .updated_files
            .contains(&"augment_offer_stats.json".to_string()));
        assert_eq!(
            fs::read(data_dir.join("augment_offer_stats.json")).unwrap(),
            b"{\"local\":1}"
        );
        assert_eq!(fs::read(data_dir.join("champions.json")).unwrap(), champs);

        server.abort();
        let _ = fs::remove_dir_all(&data_dir);
    }
}
