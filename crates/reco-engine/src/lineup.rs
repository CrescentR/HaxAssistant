use data_runtime::Dataset;
use shared_types::{BenchSuggestion, LineupEvaluation};

fn score_team(ds: &Dataset, team: &[String]) -> (f64, Vec<String>) {
    let mut delta_sum = 0.0_f64;
    let mut tanks = 0usize;
    let mut mages = 0usize;
    let mut ads = 0usize;

    for hname in team {
        if let Some(ch) = ds.find_champion(hname) {
            delta_sum += (ch.win_rate - 0.5) * 0.72;
            if ch.roles.iter().any(|r| r == "坦克" || r == "战士") {
                tanks += 1;
            }
            if ch.roles.iter().any(|r| r == "法师") {
                mages += 1;
            }
            if ch.roles.iter().any(|r| r == "射手" || r == "刺客") {
                ads += 1;
            }
        }
    }

    let mut syn_bonus = 0.0_f64;
    for i in 0..team.len() {
        if let Some(sc) = ds.hero_synergies_counters.get(&team[i]) {
            if let Some(syns) = sc.get("teammate_synergies").and_then(|v| v.as_array()) {
                for syn in syns {
                    if let Some(mate) = syn.get("teammateName").and_then(|v| v.as_str()) {
                        if team[i + 1..].iter().any(|t| t == mate) {
                            syn_bonus += syn
                                .get("synergyDelta")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0)
                                * 0.45;
                        }
                    }
                }
            }
        }
    }

    let mut notes = Vec::new();
    let mut penalty = 0.0_f64;
    if team.len() >= 4 && tanks == 0 {
        penalty += 0.024;
        notes.push("缺少前排开团与承伤 (-2.4%)".to_string());
    } else if tanks >= 1 && mages >= 1 && ads >= 1 {
        notes.push("前排/物理/魔法结构均衡 (+1.2%)".to_string());
        syn_bonus += 0.012;
    }

    if team.len() >= 4 && mages >= 4 {
        penalty += 0.022;
        notes.push("AP 伤害过于单一 (-2.2%)".to_string());
    } else if team.len() >= 4 && ads >= 4 && mages == 0 {
        penalty += 0.02;
        notes.push("纯 AD 菜刀队易被堆护甲针对 (-2.0%)".to_string());
    }

    let prob = (0.5 + delta_sum + syn_bonus - penalty).clamp(0.25, 0.75);
    (prob, notes)
}

/// Computes 5v5 lineup winrate & bench swap gain.
/// Algorithm matches `dataService.ts:evaluate5v5Lineup`.
pub fn evaluate_5v5_lineup(
    ds: &Dataset,
    ally_names: &[String],
    enemy_names: &[String],
    bench_names: &[String],
    local_hero_name: &str,
) -> LineupEvaluation {
    let (ally_prob, ally_notes) = score_team(ds, ally_names);
    let (enemy_prob, _) = score_team(ds, enemy_names);

    let mut counter_sum = 0.0_f64;
    for a in ally_names {
        if let Some(sc) = ds.hero_synergies_counters.get(a) {
            for key in ["strong_against", "weak_against"] {
                if let Some(rows) = sc.get(key).and_then(|v| v.as_array()) {
                    for row in rows {
                        if let Some(opp) = row.get("opponentName").and_then(|v| v.as_str()) {
                            if enemy_names.iter().any(|e| e == opp) {
                                counter_sum += row
                                    .get("counterDelta")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0)
                                    * 0.35;
                            }
                        }
                    }
                }
            }
        }
    }

    let base_win = (0.5 + (ally_prob - enemy_prob)).clamp(0.26, 0.74);
    let adjusted_win = (base_win + counter_sum).clamp(0.22, 0.78);
    let correction = adjusted_win - base_win;

    let mut bench_suggestions: Vec<BenchSuggestion> = bench_names
        .iter()
        .map(|bname| {
            let next_team: Vec<String> = ally_names
                .iter()
                .map(|n| {
                    if n == local_hero_name {
                        bname.clone()
                    } else {
                        n.clone()
                    }
                })
                .collect();
            let (next_prob, _) = score_team(ds, &next_team);
            let gain = next_prob - ally_prob;
            let ch = ds.find_champion(bname);
            BenchSuggestion {
                hero_name: bname.clone(),
                hero_id: ch.map(|c| c.id.clone()).unwrap_or_else(|| "22".to_string()),
                hero_win_rate: ch.map(|c| c.win_rate).unwrap_or(0.5),
                hex_score: ch.map(|c| c.hex_score).unwrap_or(60.0),
                hex_label: ch
                    .map(|c| c.hex_label.clone())
                    .unwrap_or_else(|| "人上人".to_string()),
                hex_color: ch
                    .map(|c| c.hex_tier_color.clone())
                    .unwrap_or_else(|| "#b08a2d".to_string()),
                gain,
            }
        })
        .collect();

    bench_suggestions.sort_by(|a, b| b.gain.total_cmp(&a.gain));

    LineupEvaluation {
        base_win,
        adjusted_win,
        correction,
        notes: ally_notes,
        bench_suggestions,
    }
}
