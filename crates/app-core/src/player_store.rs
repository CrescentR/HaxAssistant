//! Backend persistent store for player markers, encounter history, and augment offer statistics.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use shared_types::{
    AugmentOfferStatEntry, AugmentOfferStatsReport, PlayerEncounterRecord, PlayerEncounterSummary,
    PlayerLibraryBundle, PlayerMarker,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct OfferStatInternal {
    offered: u64,
    picked: u64,
    last_seen_epoch: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct OfferStatsStoreData {
    total_offers: u64,
    total_picks: u64,
    by_augment: HashMap<String, OfferStatInternal>,
}

pub struct PlayerStore {
    data_dir: PathBuf,
    markers: RwLock<Vec<PlayerMarker>>,
    encounters: RwLock<Vec<PlayerEncounterRecord>>,
    offer_stats: RwLock<OfferStatsStoreData>,
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn tag_label_for(tag: &str) -> String {
    match tag {
        "carry" => "大腿 · Carry".to_string(),
        "friend" => "熟人 · 固定车队".to_string(),
        "toxic" => "避雷 · 压力怪".to_string(),
        other => other.to_string(),
    }
}

fn normalize_key(s: &str) -> String {
    s.trim().to_lowercase()
}

impl PlayerStore {
    pub fn load(data_dir: &Path) -> Self {
        let markers_path = data_dir.join("player_markers.json");
        // Missing/invalid store starts empty — never seed demo players into the
        // real persistent store.
        let markers: Vec<PlayerMarker> = fs::read_to_string(&markers_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();

        let encounters_path = data_dir.join("player_encounters.json");
        let encounters: Vec<PlayerEncounterRecord> = fs::read_to_string(&encounters_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();

        let stats_path = data_dir.join("augment_offer_stats.json");
        let offer_stats: OfferStatsStoreData = fs::read_to_string(&stats_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();

        Self {
            data_dir: data_dir.to_path_buf(),
            markers: RwLock::new(markers),
            encounters: RwLock::new(encounters),
            offer_stats: RwLock::new(offer_stats),
        }
    }

    fn persist_markers(&self, items: &[PlayerMarker]) -> Result<()> {
        let _ = fs::create_dir_all(&self.data_dir);
        fs::write(
            self.data_dir.join("player_markers.json"),
            serde_json::to_string_pretty(items)?,
        )?;
        Ok(())
    }

    fn persist_encounters(&self, items: &[PlayerEncounterRecord]) -> Result<()> {
        let _ = fs::create_dir_all(&self.data_dir);
        fs::write(
            self.data_dir.join("player_encounters.json"),
            serde_json::to_string_pretty(items)?,
        )?;
        Ok(())
    }

    fn persist_offer_stats(&self, data: &OfferStatsStoreData) -> Result<()> {
        let _ = fs::create_dir_all(&self.data_dir);
        fs::write(
            self.data_dir.join("augment_offer_stats.json"),
            serde_json::to_string_pretty(data)?,
        )?;
        Ok(())
    }

    pub fn list_markers(&self) -> Vec<PlayerMarker> {
        self.markers.read().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn find_marker(&self, puuid: &str, summoner_name: &str) -> Option<PlayerMarker> {
        let p_key = normalize_key(puuid);
        let n_key = normalize_key(summoner_name);
        let guard = self.markers.read().ok()?;
        guard
            .iter()
            .find(|m| {
                (!p_key.is_empty() && normalize_key(&m.puuid) == p_key)
                    || (!n_key.is_empty() && normalize_key(&m.summoner_name) == n_key)
            })
            .cloned()
    }

    pub fn save_marker(&self, mut marker: PlayerMarker) -> Result<PlayerMarker> {
        if marker.puuid.trim().is_empty() {
            marker.puuid = format!("local-{}", now_epoch_secs());
        }
        if marker.tag_label.trim().is_empty() {
            marker.tag_label = tag_label_for(&marker.tag);
        }
        if marker.updated_at.trim().is_empty() {
            marker.updated_at = format!("epoch-{}", now_epoch_secs());
        }

        let snapshot = {
            let mut guard = self.markers.write().unwrap();
            let p_key = normalize_key(&marker.puuid);
            let n_key = normalize_key(&marker.summoner_name);
            if let Some(existing) = guard.iter_mut().find(|m| {
                normalize_key(&m.puuid) == p_key
                    || (!n_key.is_empty() && normalize_key(&m.summoner_name) == n_key)
            }) {
                *existing = marker.clone();
            } else {
                guard.insert(0, marker.clone());
            }
            guard.clone()
        };
        self.persist_markers(&snapshot)?;
        Ok(marker)
    }

    pub fn delete_marker(&self, puuid_or_name: &str) -> Result<bool> {
        let key = normalize_key(puuid_or_name);
        let (removed, snapshot) = {
            let mut guard = self.markers.write().unwrap();
            let before = guard.len();
            guard.retain(|m| {
                normalize_key(&m.puuid) != key && normalize_key(&m.summoner_name) != key
            });
            (guard.len() < before, guard.clone())
        };
        if removed {
            self.persist_markers(&snapshot)?;
        }
        Ok(removed)
    }

    pub fn record_encounters(&self, records: Vec<PlayerEncounterRecord>) -> Result<usize> {
        if records.is_empty() {
            return Ok(0);
        }
        let mut added = 0usize;
        let snapshot = {
            let mut guard = self.encounters.write().unwrap();
            for rec in records {
                if rec.summoner_name.trim().is_empty() {
                    continue;
                }
                if guard.iter().any(|existing| existing.id == rec.id) {
                    continue;
                }
                guard.insert(0, rec);
                added += 1;
            }
            if guard.len() > 2000 {
                guard.truncate(2000);
            }
            guard.clone()
        };

        if added > 0 {
            self.persist_encounters(&snapshot)?;
            // Bump encounter count on matching markers.
            let mut markers_changed = false;
            let m_snap = {
                let mut m_guard = self.markers.write().unwrap();
                for m in m_guard.iter_mut() {
                    let count = snapshot
                        .iter()
                        .filter(|e| {
                            (!m.puuid.is_empty()
                                && !e.puuid.is_empty()
                                && normalize_key(&m.puuid) == normalize_key(&e.puuid))
                                || normalize_key(&m.summoner_name)
                                    == normalize_key(&e.summoner_name)
                        })
                        .count() as u32;
                    if count > m.encounters {
                        m.encounters = count;
                        markers_changed = true;
                    }
                }
                m_guard.clone()
            };
            if markers_changed {
                let _ = self.persist_markers(&m_snap);
            }
        }
        Ok(added)
    }

    pub fn list_encounters(&self, limit: usize) -> Vec<PlayerEncounterRecord> {
        self.encounters
            .read()
            .map(|g| g.iter().take(limit.clamp(1, 500)).cloned().collect())
            .unwrap_or_default()
    }

    pub fn encounter_summary(&self, query: &str) -> PlayerEncounterSummary {
        let key = normalize_key(query);
        let matches: Vec<PlayerEncounterRecord> = self
            .encounters
            .read()
            .map(|g| {
                g.iter()
                    .filter(|e| {
                        normalize_key(&e.puuid) == key
                            || normalize_key(&e.summoner_name) == key
                            || (!key.is_empty() && normalize_key(&e.summoner_name).contains(&key))
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        let ally_games = matches.iter().filter(|e| e.relation == "ally").count();
        let enemy_games = matches.iter().filter(|e| e.relation == "enemy").count();
        let ally_wins = matches
            .iter()
            .filter(|e| e.relation == "ally" && e.result == "WIN")
            .count();
        let enemy_wins = matches
            .iter()
            .filter(|e| e.relation == "enemy" && e.result == "WIN")
            .count();

        PlayerEncounterSummary {
            query: query.to_string(),
            total_encounters: matches.len(),
            ally_games,
            enemy_games,
            ally_wins,
            enemy_wins,
            marker: self.find_marker(query, query),
            recent: matches.into_iter().take(20).collect(),
        }
    }

    pub fn delete_encounters(&self, puuid_or_name: &str) -> Result<usize> {
        let key = normalize_key(puuid_or_name);
        let (removed, snapshot) = {
            let mut guard = self.encounters.write().unwrap();
            let before = guard.len();
            guard.retain(|e| {
                normalize_key(&e.id) != key
                    && normalize_key(&e.puuid) != key
                    && normalize_key(&e.summoner_name) != key
            });
            (before - guard.len(), guard.clone())
        };
        if removed > 0 {
            self.persist_encounters(&snapshot)?;
        }
        Ok(removed)
    }

    pub fn prune_encounters(&self, max_age_days: u64) -> Result<usize> {
        let cutoff = now_epoch_secs().saturating_sub(max_age_days.saturating_mul(86_400));
        let (removed, snapshot) = {
            let mut guard = self.encounters.write().unwrap();
            let before = guard.len();
            guard.retain(|e| e.timestamp >= cutoff);
            (before - guard.len(), guard.clone())
        };
        if removed > 0 {
            self.persist_encounters(&snapshot)?;
        }
        Ok(removed)
    }

    pub fn export_library(&self) -> PlayerLibraryBundle {
        PlayerLibraryBundle {
            schema_version: "1".to_string(),
            exported_at: now_epoch_secs(),
            markers: self.list_markers(),
            encounters: self.encounters.read().map(|g| g.clone()).unwrap_or_default(),
        }
    }

    pub fn import_library(&self, bundle: PlayerLibraryBundle, replace: bool) -> Result<(usize, usize)> {
        let m_snap = {
            let mut guard = self.markers.write().unwrap();
            if replace {
                *guard = bundle.markers;
            } else {
                for incoming in bundle.markers {
                    let p_key = normalize_key(&incoming.puuid);
                    let n_key = normalize_key(&incoming.summoner_name);
                    if let Some(existing) = guard.iter_mut().find(|m| {
                        normalize_key(&m.puuid) == p_key
                            || (!n_key.is_empty() && normalize_key(&m.summoner_name) == n_key)
                    }) {
                        *existing = incoming;
                    } else {
                        guard.push(incoming);
                    }
                }
            }
            guard.clone()
        };
        self.persist_markers(&m_snap)?;

        let e_snap = {
            let mut guard = self.encounters.write().unwrap();
            if replace {
                *guard = bundle.encounters;
            } else {
                for incoming in bundle.encounters {
                    if !guard.iter().any(|e| e.id == incoming.id) {
                        guard.push(incoming);
                    }
                }
            }
            guard.clone()
        };
        self.persist_encounters(&e_snap)?;
        Ok((m_snap.len(), e_snap.len()))
    }

    // -----------------------------------------------------------------------
    // Augment offer / pick occurrence stats
    // -----------------------------------------------------------------------

    pub fn record_augment_offer(&self, offered: &[String]) {
        if offered.is_empty() {
            return;
        }
        let now = now_epoch_secs();
        let snap = {
            let mut guard = self.offer_stats.write().unwrap();
            guard.total_offers += 1;
            for name in offered {
                let trimmed = name.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let entry = guard.by_augment.entry(trimmed.to_string()).or_default();
                entry.offered += 1;
                entry.last_seen_epoch = now;
            }
            guard.clone()
        };
        let _ = self.persist_offer_stats(&snap);
    }

    pub fn record_augment_pick(&self, augment_name: &str) {
        let trimmed = augment_name.trim();
        if trimmed.is_empty() {
            return;
        }
        let now = now_epoch_secs();
        let snap = {
            let mut guard = self.offer_stats.write().unwrap();
            guard.total_picks += 1;
            let entry = guard.by_augment.entry(trimmed.to_string()).or_default();
            entry.picked += 1;
            if entry.offered < entry.picked {
                entry.offered = entry.picked;
            }
            entry.last_seen_epoch = now;
            guard.clone()
        };
        let _ = self.persist_offer_stats(&snap);
    }

    pub fn get_offer_stats(&self, top_n: usize) -> AugmentOfferStatsReport {
        let guard = self.offer_stats.read().unwrap();
        let mut rows: Vec<AugmentOfferStatEntry> = guard
            .by_augment
            .iter()
            .map(|(name, st)| AugmentOfferStatEntry {
                augment_name: name.clone(),
                offered: st.offered,
                picked: st.picked,
                pick_rate: if st.offered > 0 {
                    ((st.picked as f64 / st.offered as f64) * 1000.0).round() / 1000.0
                } else {
                    0.0
                },
                last_seen_epoch: st.last_seen_epoch,
            })
            .collect();
        rows.sort_by(|a, b| {
            b.offered
                .cmp(&a.offered)
                .then_with(|| b.picked.cmp(&a.picked))
        });
        rows.truncate(top_n.clamp(1, 200));

        AugmentOfferStatsReport {
            total_offers: guard.total_offers,
            total_picks: guard.total_picks,
            unique_augments: guard.by_augment.len(),
            top_offered: rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hexbox_player_store_{tag}_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saves_deletes_and_roundtrips_library_bundle() {
        let dir = temp_dir("lib");
        let store = PlayerStore::load(&dir);

        let saved = store
            .save_marker(PlayerMarker {
                puuid: "p-42".into(),
                summoner_name: "测试玩家#KR1".into(),
                tag: "carry".into(),
                tag_label: String::new(),
                note: "极强".into(),
                encounters: 1,
                updated_at: String::new(),
            })
            .unwrap();
        assert_eq!(saved.tag_label, "大腿 · Carry");
        assert!(store.find_marker("p-42", "").is_some());

        store
            .record_encounters(vec![PlayerEncounterRecord {
                id: "g1:p-42".into(),
                puuid: "p-42".into(),
                summoner_name: "测试玩家#KR1".into(),
                champion_name: "寒冰射手".into(),
                relation: "ally".into(),
                result: "WIN".into(),
                kda: "12/3/20".into(),
                timestamp: now_epoch_secs(),
                time_label: "刚刚".into(),
            }])
            .unwrap();

        let summary = store.encounter_summary("测试玩家#KR1");
        assert_eq!(summary.total_encounters, 1);
        assert_eq!(summary.ally_wins, 1);

        let exported = store.export_library();
        assert!(exported.markers.iter().any(|m| m.puuid == "p-42"));
        assert_eq!(exported.encounters.len(), 1);

        let dir2 = temp_dir("lib_import");
        let store2 = PlayerStore::load(&dir2);
        let (m_count, e_count) = store2.import_library(exported, true).unwrap();
        assert!(m_count >= 1);
        assert_eq!(e_count, 1);

        assert!(store2.delete_marker("p-42").unwrap());
        assert!(store2.find_marker("p-42", "").is_none());

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dir2);
    }

    #[test]
    fn tracks_augment_offer_and_pick_stats() {
        let dir = temp_dir("offer_stats");
        let store = PlayerStore::load(&dir);
        store.record_augment_offer(&["易损".into(), "亮出你的剑".into(), "暴击飞弹".into()]);
        store.record_augment_pick("亮出你的剑");
        let report = store.get_offer_stats(10);
        assert_eq!(report.total_offers, 1);
        assert_eq!(report.total_picks, 1);
        let sword = report
            .top_offered
            .iter()
            .find(|r| r.augment_name == "亮出你的剑")
            .unwrap();
        assert_eq!(sword.offered, 1);
        assert_eq!(sword.picked, 1);
        assert!((sword.pick_rate - 1.0).abs() < 0.001);
        let _ = fs::remove_dir_all(&dir);
    }
}
