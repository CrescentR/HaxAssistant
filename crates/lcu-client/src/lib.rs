use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use base64::Engine;
use serde_json::Value;
use shared_types::{
    ChampSelectSnapshot, LcuStatus, LiveGameSnapshot, LivePlayerScore, MatchHistoryEntry,
};

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

    pub async fn fetch_match_history(
        &self,
        creds: &LockfileCredentials,
        count: usize,
    ) -> Result<Value> {
        let end_index = count.clamp(1, 50);
        let endpoint = format!(
            "/lol-match-history/v1/products/lol/current-summoner/matches?begIndex=0&endIndex={end_index}"
        );
        self.get_json(creds, &endpoint).await
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

pub fn format_time_ago(game_creation_ms: u64) -> String {
    if game_creation_ms == 0 {
        return "近期".to_string();
    }
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(game_creation_ms);
    let diff_secs = now_ms.saturating_sub(game_creation_ms) / 1000;
    if diff_secs < 120 {
        "刚刚".to_string()
    } else if diff_secs < 3600 {
        format!("{} 分钟前", diff_secs / 60)
    } else if diff_secs < 86_400 {
        format!("{} 小时前", diff_secs / 3600)
    } else {
        format!("{} 天前", diff_secs / 86_400)
    }
}

pub fn parse_match_history(
    raw: &Value,
    resolve_hero: impl Fn(u32) -> String,
    resolve_augment: impl Fn(u32) -> Option<String>,
) -> Vec<MatchHistoryEntry> {
    let games = raw
        .get("games")
        .and_then(|g| g.get("games").or(Some(g)))
        .and_then(|v| v.as_array());

    let Some(games) = games else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for game in games {
        let game_id = game
            .get("gameId")
            .and_then(|v| v.as_u64())
            .map(|id| id.to_string())
            .unwrap_or_else(|| "0".to_string());
        let queue_id = game.get("queueId").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let game_mode = game
            .get("gameMode")
            .and_then(|v| v.as_str())
            .unwrap_or("ARAM")
            .to_string();
        let duration_secs = game
            .get("gameDuration")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let game_creation = game
            .get("gameCreation")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let Some(participant) = game
            .get("participants")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
        else {
            continue;
        };

        let champion_id = participant
            .get("championId")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let hero_name = resolve_hero(champion_id);

        let stats = participant.get("stats").unwrap_or(&Value::Null);
        let win = stats.get("win").and_then(|v| v.as_bool()).unwrap_or(false);
        let kills = stats.get("kills").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let deaths = stats.get("deaths").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let assists = stats.get("assists").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let damage = stats
            .get("totalDamageDealtToChampions")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let mut augments = Vec::new();
        for key in [
            "playerAugment1",
            "playerAugment2",
            "playerAugment3",
            "playerAugment4",
        ] {
            if let Some(aid) = stats.get(key).and_then(|v| v.as_u64()).map(|n| n as u32) {
                if aid > 0 {
                    if let Some(name) = resolve_augment(aid) {
                        augments.push(name);
                    } else {
                        augments.push(format!("强化 #{aid}"));
                    }
                }
            }
        }

        let kda_ratio = (kills as f64 + assists as f64) / (deaths.max(1) as f64);
        let dmg_bonus = ((damage as f64) / 5000.0).min(12.0);
        let win_bonus = if win { 11.0 } else { -7.0 };
        let hex_score = (56.0 + win_bonus + (kda_ratio * 3.8).min(18.0) + dmg_bonus)
            .clamp(32.0, 95.0);
        let hex_score = (hex_score * 10.0).round() / 10.0;

        let duration = format!("{:02}:{:02}", duration_secs / 60, duration_secs % 60);
        out.push(MatchHistoryEntry {
            game_id,
            champion_id,
            hero_name,
            result: if win { "WIN" } else { "LOSS" }.to_string(),
            kills,
            deaths,
            assists,
            kda: format!("{kills} / {deaths} / {assists}"),
            damage,
            duration_secs,
            duration,
            queue_id,
            game_mode,
            augments,
            hex_score,
            game_creation,
            time_ago: format_time_ago(game_creation),
        });
    }

    out
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

    #[test]
    fn parses_lcu_match_history() {
        let raw = json!({
            "games": {
                "games": [
                    {
                        "gameId": 987654321,
                        "queueId": 450,
                        "gameMode": "ARAM",
                        "gameDuration": 1062,
                        "gameCreation": 0,
                        "participants": [
                            {
                                "championId": 22,
                                "stats": {
                                    "win": true,
                                    "kills": 12,
                                    "deaths": 4,
                                    "assists": 22,
                                    "totalDamageDealtToChampions": 51200,
                                    "playerAugment1": 101,
                                    "playerAugment2": 102
                                }
                            }
                        ]
                    }
                ]
            }
        });

        let entries = parse_match_history(
            &raw,
            |cid| if cid == 22 { "寒冰射手".to_string() } else { cid.to_string() },
            |aid| match aid {
                101 => Some("亮出你的剑".to_string()),
                102 => Some("易损".to_string()),
                _ => None,
            },
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].hero_name, "寒冰射手");
        assert_eq!(entries[0].result, "WIN");
        assert_eq!(entries[0].kda, "12 / 4 / 22");
        assert_eq!(entries[0].duration, "17:42");
        assert_eq!(entries[0].augments, vec!["亮出你的剑", "易损"]);
        assert!(entries[0].hex_score >= 70.0);
    }
}
