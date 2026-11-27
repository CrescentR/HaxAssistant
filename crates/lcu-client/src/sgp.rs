//! Riot SGP (Service Gateway Proxy) client for cross-player match history queries.
//! Uses the local LCU league-session token + region to query SGP match history,
//! with transparent fallback to LCU endpoints when SGP is unavailable.

use anyhow::{anyhow, Result};
use serde_json::Value;
use shared_types::MatchHistoryEntry;

use crate::{format_time_ago, LcuClient, LockfileCredentials};

/// Map Riot platform/region identifiers to their SGP match-history host.
pub fn sgp_host_for_region(region: &str) -> Option<&'static str> {
    let norm = region.trim().to_ascii_uppercase();
    match norm.as_str() {
        "HN1" | "HN2" | "HN3" | "HN4" | "HN5" | "HN6" | "HN7" | "HN8" | "HN9" | "HN10"
        | "HN11" | "HN12" | "HN13" | "HN14" | "HN15" | "HN16" | "HN17" | "HN18" | "HN19"
        | "WT1" | "WT2" | "WT3" | "WT4" | "WT5" | "WT6" | "WT7" | "BGP1" | "BGP2" | "EDU1"
        | "TENCENT" => Some("https://hn1-k8s-sgp.lol.qq.com:21019"),
        "KR" => Some("https://kr-sgp.lol.sgp.pvp.net"),
        "JP" | "JP1" => Some("https://jp1-sgp.lol.sgp.pvp.net"),
        "NA" | "NA1" => Some("https://na2-sgp.lol.sgp.pvp.net"),
        "EUW" | "EUW1" => Some("https://euw1-sgp.lol.sgp.pvp.net"),
        "EUNE" | "EUN1" => Some("https://eun1-sgp.lol.sgp.pvp.net"),
        "OC1" | "OCE" => Some("https://oc1-sgp.lol.sgp.pvp.net"),
        "TW2" => Some("https://tw2-sgp.lol.sgp.pvp.net"),
        "SG2" => Some("https://sg2-sgp.lol.sgp.pvp.net"),
        "VN2" => Some("https://vn2-sgp.lol.sgp.pvp.net"),
        _ => None,
    }
}

/// Normalize LCU locale strings (e.g. `ko_KR`, `en_US`, `zh_CN`) to OCR lang codes.
pub fn ocr_lang_from_locale(locale: &str) -> &'static str {
    let lower = locale.trim().to_ascii_lowercase();
    if lower.starts_with("ko") {
        "ko"
    } else if lower.starts_with("en") {
        "en"
    } else {
        "ch"
    }
}

impl LcuClient {
    /// Query SGP match history by PUUID using LCU's league-session token.
    pub async fn fetch_sgp_match_history(
        &self,
        creds: &LockfileCredentials,
        puuid: &str,
        count: usize,
    ) -> Result<Value> {
        let token_val = self
            .get_json(creds, "/lol-league-session/v1/league-session-token")
            .await?;
        let token = token_val
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("empty league-session-token"))?;

        let region_info = self
            .get_json(creds, "/riotclient/region-locale")
            .await
            .unwrap_or(Value::Null);
        let region = region_info
            .get("webRegion")
            .or_else(|| region_info.get("region"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("cannot determine client region for SGP query"))?;

        let host = sgp_host_for_region(region)
            .ok_or_else(|| anyhow!("unsupported SGP region: {region}"))?;
        let limit = count.clamp(1, 30);
        let url = format!(
            "{host}/match-history-query/v1/products/lol/player/{puuid}/SUMMARY?startIndex=0&count={limit}"
        );
        // Use a cert-verifying client: this request carries the league-session
        // bearer token to a public host.
        let resp = self
            .secure_http_client()
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(anyhow!("SGP query failed: {}", resp.status()));
        }
        Ok(resp.json::<Value>().await?)
    }
}

/// Parse SGP `SUMMARY` response (`{ "games": [ { "json": { ... } } ] }`) into `MatchHistoryEntry`.
pub fn parse_sgp_match_history<FChamp, FAug>(
    raw: &Value,
    target_puuid: &str,
    resolve_champion: FChamp,
    resolve_augment: FAug,
) -> Vec<MatchHistoryEntry>
where
    FChamp: Fn(u32) -> String,
    FAug: Fn(u32) -> Option<String>,
{
    let Some(games) = raw.get("games").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for g in games {
        let payload = g.get("json").unwrap_or(g);
        let game_id = payload
            .get("gameId")
            .and_then(|v| v.as_u64())
            .map(|id| id.to_string())
            .or_else(|| {
                g.get("metadata")
                    .and_then(|m| m.get("match_id"))
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string)
            })
            .unwrap_or_default();
        if game_id.is_empty() {
            continue;
        }
        let duration_secs = payload
            .get("gameDuration")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let queue_id = payload.get("queueId").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let game_mode = payload
            .get("gameMode")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let game_creation = payload
            .get("gameCreation")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let Some(participants) = payload.get("participants").and_then(|v| v.as_array()) else {
            continue;
        };
        // SUMMARY carries all ten players; only the queried PUUID's row is
        // theirs. Never fall back to another participant.
        let Some(p) = participants
            .iter()
            .find(|p| p.get("puuid").and_then(|v| v.as_str()) == Some(target_puuid))
        else {
            continue;
        };

        let champion_id = p.get("championId").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        if champion_id == 0 {
            continue;
        }
        let hero_name = resolve_champion(champion_id);
        let Some(win) = p.get("win").and_then(|v| v.as_bool()) else {
            continue;
        };
        let kills = p.get("kills").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let deaths = p.get("deaths").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let assists = p.get("assists").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let damage = p
            .get("totalDamageDealtToChampions")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let damage_taken = p
            .get("totalDamageTaken")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let heal = p.get("totalHeal").and_then(|v| v.as_u64()).unwrap_or(0);

        let mut items = Vec::new();
        for key in ["item0", "item1", "item2", "item3", "item4", "item5", "item6"] {
            if let Some(id) = p.get(key).and_then(|v| v.as_u64()) {
                if id > 0 {
                    items.push(id as u32);
                }
            }
        }

        let mut augments = Vec::new();
        for key in [
            "playerAugment1",
            "playerAugment2",
            "playerAugment3",
            "playerAugment4",
            "playerAugment5",
            "playerAugment6",
        ] {
            if let Some(aid) = p.get(key).and_then(|v| v.as_u64()) {
                if aid > 0 {
                    if let Some(name) = resolve_augment(aid as u32) {
                        augments.push(name);
                    }
                }
            }
        }

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
            damage_taken,
            heal,
            items,
            duration_secs,
            duration: format!("{:02}:{:02}", duration_secs / 60, duration_secs % 60),
            queue_id,
            game_mode,
            augments,
            // Filled in from dataset augment ratings by app-core.
            hex_score: None,
            game_creation,
            time_ago: format_time_ago(game_creation),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_regions_and_locales() {
        assert!(sgp_host_for_region("HN1").is_some());
        assert!(sgp_host_for_region("kr").is_some());
        assert_eq!(ocr_lang_from_locale("ko_KR"), "ko");
        assert_eq!(ocr_lang_from_locale("en_US"), "en");
        assert_eq!(ocr_lang_from_locale("zh_CN"), "ch");
    }

    #[test]
    fn parses_sgp_summary_games() {
        let raw = serde_json::json!({
            "games": [{
                "json": {
                    "gameId": 888999,
                    "gameDuration": 1020,
                    "queueId": 450,
                    "gameMode": "ARAM",
                    "gameCreation": 0,
                    "participants": [{
                        "puuid": "p-1",
                        "championId": 22,
                        "win": true,
                        "kills": 12,
                        "deaths": 4,
                        "assists": 18,
                        "totalDamageDealtToChampions": 48000,
                        "totalDamageTaken": 19000,
                        "totalHeal": 3200,
                        "item0": 3032,
                        "item1": 3085,
                        "playerAugment1": 101
                    }]
                }
            }]
        });
        let list = parse_sgp_match_history(
            &raw,
            "p-1",
            |id| if id == 22 { "寒冰射手".into() } else { format!("#{id}") },
            |aid| if aid == 101 { Some("易损".into()) } else { None },
        );
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].hero_name, "寒冰射手");
        assert_eq!(list[0].items, vec![3032, 3085]);
        assert_eq!(list[0].augments, vec!["易损"]);
        // No KDA/result-derived score and no invented "recent" label.
        assert_eq!(list[0].hex_score, None);
        assert_eq!(list[0].duration, "17:00");
        assert_eq!(list[0].time_ago, "");
    }

    /// SUMMARY games list all ten players; only the queried PUUID's row may be
    /// used, and a game where that PUUID is absent must be dropped instead of
    /// being attributed to `participants[0]`.
    #[test]
    fn sgp_history_scopes_to_target_puuid() {
        let raw = serde_json::json!({
            "games": [
                {
                    "metadata": { "match_id": "HN1_1" },
                    "json": {
                        "gameId": 1,
                        "gameDuration": 1200,
                        "queueId": 2400,
                        "gameMode": "KIWI",
                        "gameCreation": 1_700_000_000_000u64,
                        "participants": [
                            { "puuid": "enemy", "championId": 99, "win": false,
                              "kills": 2, "deaths": 8, "assists": 3 },
                            { "puuid": "me", "championId": 157, "win": true,
                              "kills": 9, "deaths": 2, "assists": 11 }
                        ]
                    }
                },
                {
                    "json": {
                        "gameId": 2,
                        "participants": [
                            { "puuid": "someone-else", "championId": 22, "win": true }
                        ]
                    }
                },
                {
                    // Missing outcome → skipped, not shown as a loss.
                    "json": {
                        "gameId": 3,
                        "participants": [ { "puuid": "me", "championId": 22 } ]
                    }
                }
            ]
        });
        let list = parse_sgp_match_history(&raw, "me", |id| format!("#{id}"), |_| None);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].game_id, "1");
        assert_eq!(list[0].champion_id, 157);
        assert_eq!(list[0].result, "WIN");
        assert_eq!(list[0].kda, "9 / 2 / 11");
        assert_eq!(list[0].game_mode, "KIWI");
        assert!(!list[0].time_ago.is_empty());
    }

    #[test]
    fn sgp_history_handles_empty_or_error_payloads() {
        let champ = |c: u32| c.to_string();
        let none = |_: u32| -> Option<String> { None };
        for raw in [
            serde_json::json!({}),
            serde_json::json!({ "games": [] }),
            serde_json::json!({ "httpStatus": 401, "message": "unauthorized" }),
        ] {
            assert!(parse_sgp_match_history(&raw, "me", champ, none).is_empty());
        }
    }
}
