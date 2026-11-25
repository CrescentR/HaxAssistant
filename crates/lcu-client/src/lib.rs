use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Result};
use base64::Engine;
use serde_json::Value;
use shared_types::{ChampSelectSnapshot, LcuStatus, LiveGameSnapshot, LivePlayerScore};

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

    pub async fn fetch_champ_select(
        &self,
        creds: &LockfileCredentials,
    ) -> Result<ChampSelectSnapshot> {
        let session = self.get_json(creds, "/lol-champ-select/v1/session").await?;
        Ok(parse_champ_select_session(&session))
    }

    pub async fn fetch_live_game_snapshot(&self) -> Result<LiveGameSnapshot> {
        let data = self.fetch_live_client_data().await?;
        Ok(parse_live_game_data(&data))
    }
}

pub fn parse_champ_select_session(session: &Value) -> ChampSelectSnapshot {
    let local_cell = session
        .get("localPlayerCellId")
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);

    let mut my_champion_id = None;
    let mut ally_champion_ids = Vec::new();
    if let Some(my_team) = session.get("myTeam").and_then(|v| v.as_array()) {
        for member in my_team {
            let champ_id = member
                .get("championId")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            if champ_id > 0 {
                ally_champion_ids.push(champ_id);
            }
            if member.get("cellId").and_then(|v| v.as_i64()) == Some(local_cell) && champ_id > 0 {
                my_champion_id = Some(champ_id);
            }
        }
    }

    let mut enemy_champion_ids = Vec::new();
    if let Some(their_team) = session.get("theirTeam").and_then(|v| v.as_array()) {
        for member in their_team {
            let champ_id = member
                .get("championId")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            if champ_id > 0 {
                enemy_champion_ids.push(champ_id);
            }
        }
    }

    let mut bench_champion_ids = Vec::new();
    if let Some(bench) = session.get("benchChampions").and_then(|v| v.as_array()) {
        for entry in bench {
            if let Some(cid) = entry.get("championId").and_then(|v| v.as_u64()) {
                if cid > 0 {
                    bench_champion_ids.push(cid as u32);
                }
            }
        }
    }

    let timer_phase = session
        .get("timer")
        .and_then(|t| t.get("phase"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    ChampSelectSnapshot {
        my_champion_id,
        ally_champion_ids,
        enemy_champion_ids,
        bench_champion_ids,
        timer_phase,
    }
}

pub fn parse_live_game_data(data: &Value) -> LiveGameSnapshot {
    let active_name = data
        .get("activePlayer")
        .and_then(|ap| ap.get("summonerName").or_else(|| ap.get("riotIdGameName")))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let current_gold = data
        .get("activePlayer")
        .and_then(|ap| ap.get("currentGold"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let game_time = data
        .get("gameData")
        .and_then(|g| g.get("gameTime"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let mut active_champion = None;
    let mut active_team = "ORDER".to_string();
    let mut is_dead = false;
    let mut players = Vec::new();

    if let Some(all_players) = data.get("allPlayers").and_then(|v| v.as_array()) {
        for p in all_players {
            let summoner = p
                .get("summonerName")
                .or_else(|| p.get("riotIdGameName"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let champion = p
                .get("championName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let team = p
                .get("team")
                .and_then(|v| v.as_str())
                .unwrap_or("ORDER")
                .to_string();
            let dead = p.get("isDead").and_then(|v| v.as_bool()).unwrap_or(false);
            let scores = p.get("scores");
            let kills = scores
                .and_then(|s| s.get("kills"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let deaths = scores
                .and_then(|s| s.get("deaths"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let assists = scores
                .and_then(|s| s.get("assists"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;

            if !active_name.is_empty() && summoner == active_name {
                active_champion = Some(champion.clone());
                active_team = team.clone();
                is_dead = dead;
            }

            players.push(LivePlayerScore {
                summoner_name: summoner,
                champion_name: champion,
                team,
                kills,
                deaths,
                assists,
                is_dead: dead,
            });
        }
    }

    let mut allies = Vec::new();
    let mut enemies = Vec::new();
    for p in &players {
        if p.team == active_team {
            allies.push(p.champion_name.clone());
        } else {
            enemies.push(p.champion_name.clone());
        }
    }

    LiveGameSnapshot {
        in_game: !players.is_empty(),
        active_champion,
        current_gold,
        is_dead,
        game_time,
        allies,
        enemies,
        players,
    }
}

pub fn parse_lockfile_at(path: &Path) -> Result<LockfileCredentials> {
    let content = fs::read_to_string(path)?;
    LockfileCredentials::parse(&content, path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_lockfile_credentials() {
        let creds = LockfileCredentials::parse(
            "LeagueClient:12345:54321:secret_pw:https",
            PathBuf::from("/tmp/lockfile"),
        )
        .expect("parse lockfile");
        assert_eq!(creds.pid, 12345);
        assert_eq!(creds.port, 54321);
        assert_eq!(creds.base_url(), "https://127.0.0.1:54321");
        assert!(creds.auth_header().starts_with("Basic "));
    }

    #[test]
    fn parses_champ_select_and_bench() {
        let session = json!({
            "localPlayerCellId": 2,
            "myTeam": [
                { "cellId": 1, "championId": 99 },
                { "cellId": 2, "championId": 22 }
            ],
            "theirTeam": [
                { "cellId": 6, "championId": 157 }
            ],
            "benchChampions": [
                { "championId": 4 },
                { "championId": 114 }
            ],
            "timer": { "phase": "FINALIZATION" }
        });
        let snap = parse_champ_select_session(&session);
        assert_eq!(snap.my_champion_id, Some(22));
        assert_eq!(snap.ally_champion_ids, vec![99, 22]);
        assert_eq!(snap.enemy_champion_ids, vec![157]);
        assert_eq!(snap.bench_champion_ids, vec![4, 114]);
        assert_eq!(snap.timer_phase, "FINALIZATION");
    }

    #[test]
    fn parses_live_game_snapshot() {
        let data = json!({
            "activePlayer": { "summonerName": "PlayerA", "currentGold": 3150.5 },
            "gameData": { "gameTime": 612.0 },
            "allPlayers": [
                {
                    "summonerName": "PlayerA",
                    "championName": "寒冰射手",
                    "team": "ORDER",
                    "isDead": true,
                    "scores": { "kills": 8, "deaths": 3, "assists": 12 }
                },
                {
                    "summonerName": "EnemyB",
                    "championName": "疾风剑豪",
                    "team": "CHAOS",
                    "isDead": false,
                    "scores": { "kills": 5, "deaths": 6, "assists": 4 }
                }
            ]
        });
        let snap = parse_live_game_data(&data);
        assert!(snap.in_game);
        assert_eq!(snap.active_champion.as_deref(), Some("寒冰射手"));
        assert!(snap.is_dead);
        assert_eq!(snap.current_gold, 3150.5);
        assert_eq!(snap.allies, vec!["寒冰射手"]);
        assert_eq!(snap.enemies, vec!["疾风剑豪"]);
        assert_eq!(snap.players.len(), 2);
    }
}
