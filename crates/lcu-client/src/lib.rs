use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Result};
use base64::Engine;
use serde_json::Value;
use shared_types::LcuStatus;

#[derive(Debug, Clone)]
pub struct LockfileCredentials {
    pub process_name: String,
    pub pid: u32,
    pub port: u16,
    pub password: String,
    pub protocol: String,
    pub path: PathBuf,
}

impl LockfileCredentials {
    pub fn parse(content: &str, path: PathBuf) -> Result<Self> {
        let parts: Vec<&str> = content.trim().split(':').collect();
        if parts.len() < 5 {
            return Err(anyhow!("invalid LeagueClient lockfile format"));
        }
        Ok(Self {
            process_name: parts[0].to_string(),
            pid: parts[1].parse().unwrap_or(0),
            port: parts[2].parse()?,
            password: parts[3].to_string(),
            protocol: parts[4].to_string(),
            path,
        })
    }

    pub fn base_url(&self) -> String {
        format!("{}://127.0.0.1:{}", self.protocol, self.port)
    }

    pub fn auth_header(&self) -> String {
        let raw = format!("riot:{}", self.password);
        let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
        format!("Basic {encoded}")
    }
}

pub fn candidate_lockfile_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(env_path) = std::env::var("HEXBOX_LCU_LOCKFILE") {
        paths.push(PathBuf::from(env_path));
    }

    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from(
            "/Applications/League of Legends.app/Contents/LoL/lockfile",
        ));
        if let Ok(home) = std::env::var("HOME") {
            paths.push(
                PathBuf::from(home)
                    .join("Applications/League of Legends.app/Contents/LoL/lockfile"),
            );
        }
    }

    #[cfg(target_os = "windows")]
    {
        for drive in ["C", "D", "E", "F"] {
            paths.push(PathBuf::from(format!(
                "{drive}:\\Riot Games\\League of Legends\\lockfile"
            )));
            paths.push(PathBuf::from(format!(
                "{drive}:\\WeGameApps\\英雄联盟\\LeagueClient\\lockfile"
            )));
            paths.push(PathBuf::from(format!(
                "{drive}:\\Program Files\\腾讯游戏\\英雄联盟\\LeagueClient\\lockfile"
            )));
        }
    }

    paths
}

pub fn discover_lockfile() -> Option<LockfileCredentials> {
    for path in candidate_lockfile_paths() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(creds) = LockfileCredentials::parse(&content, path) {
                return Some(creds);
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct LcuClient {
    http: reqwest::Client,
}

impl Default for LcuClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LcuClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_millis(2500))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { http }
    }

    pub async fn probe_status(&self) -> LcuStatus {
        let Some(creds) = discover_lockfile() else {
            return LcuStatus {
                connected: false,
                phase: "Disconnected".to_string(),
                port: None,
                summoner_name: None,
                lockfile_path: None,
            };
        };

        let phase = self
            .get_json(&creds, "/lol-gameflow/v1/gameflow-phase")
            .await
            .ok()
            .and_then(|v| v.as_str().map(ToString::to_string))
            .unwrap_or_else(|| "None".to_string());

        let summoner_name = self
            .get_json(&creds, "/lol-summoner/v1/current-summoner")
            .await
            .ok()
            .and_then(|v| {
                v.get("gameName")
                    .or_else(|| v.get("displayName"))
                    .and_then(|n| n.as_str())
                    .map(ToString::to_string)
            });

        LcuStatus {
            connected: true,
            phase,
            port: Some(creds.port),
            summoner_name,
            lockfile_path: Some(creds.path.display().to_string()),
        }
    }

    pub async fn get_json(&self, creds: &LockfileCredentials, endpoint: &str) -> Result<Value> {
        let url = format!("{}{}", creds.base_url(), endpoint);
        let res = self
            .http
            .get(&url)
            .header("Authorization", creds.auth_header())
            .send()
            .await?;
        let status = res.status();
        if !status.is_success() {
            return Err(anyhow!("LCU GET {endpoint} failed: {status}"));
        }
        Ok(res.json::<Value>().await?)
    }

    pub async fn accept_ready_check(&self, creds: &LockfileCredentials) -> Result<()> {
        let url = format!("{}/lol-matchmaking/v1/ready-check/accept", creds.base_url());
        let res = self
            .http
            .post(&url)
            .header("Authorization", creds.auth_header())
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(anyhow!("ready-check accept failed: {}", res.status()));
        }
        Ok(())
    }

    pub async fn bench_swap(&self, creds: &LockfileCredentials, champion_id: u32) -> Result<()> {
        let url = format!(
            "{}/lol-champ-select/v1/session/bench/swap/{champion_id}",
            creds.base_url()
        );
        let res = self
            .http
            .post(&url)
            .header("Authorization", creds.auth_header())
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(anyhow!("bench swap {champion_id} failed: {}", res.status()));
        }
        Ok(())
    }

    pub async fn fetch_live_client_data(&self) -> Result<Value> {
        let res = self
            .http
            .get("https://127.0.0.1:2999/liveclientdata/allgamedata")
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(anyhow!("liveclientdata failed: {}", res.status()));
        }
        Ok(res.json::<Value>().await?)
    }
}

pub fn parse_lockfile_at(path: &Path) -> Result<LockfileCredentials> {
    let content = fs::read_to_string(path)?;
    LockfileCredentials::parse(&content, path.to_path_buf())
}
