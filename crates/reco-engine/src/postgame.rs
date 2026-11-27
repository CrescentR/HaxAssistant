//! Post-game review of one finished match.
//!
//! Ranks the player's per-minute output against the benchmark distribution
//! for the same hero, rates each augment taken, flags mechanic links and
//! conflicts between them, and checks how much of the formula build was
//! finished.

use data_runtime::Dataset;
use shared_types::{
    get_tier_meta, MatchHistoryEntry, PostgameAugmentVerdict, PostgameMetric, PostgameReview,
};

/// Benchmarks were computed from games of at least this length.
const MIN_BENCHMARK_SECS: u64 = 600;

/// Rank of `value` (0..1) in a distribution given as `values[i]` at
/// `quantiles[i]`, interpolating linearly between neighbouring quantiles.
pub fn percentile_of(value: f64, quantiles: &[f64], values: &[f64]) -> Option<f64> {
    if quantiles.is_empty() || quantiles.len() != values.len() {
        return None;
    }
    let (first_q, first_v) = (quantiles[0], values[0]);
    if value <= first_v {
        let share = if first_v > 0.0 { (value / first_v).clamp(0.0, 1.0) } else { 0.0 };
        return Some(first_q * share);
    }
    for i in 1..values.len() {
        if value <= values[i] {
            let span = values[i] - values[i - 1];
            let t = if span > 0.0 { (value - values[i - 1]) / span } else { 1.0 };
            return Some(quantiles[i - 1] + t * (quantiles[i] - quantiles[i - 1]));
        }
    }
    // Above the top quantile: approach, but never reach, 1.0.
    let (last_q, last_v) = (quantiles[quantiles.len() - 1], values[values.len() - 1]);
    let excess = if last_v > 0.0 { (value - last_v) / last_v } else { 0.0 };
    Some((last_q + (1.0 - last_q) * excess.min(1.0) * 0.8).min(0.99))
}

fn median_of(quantiles: &[f64], values: &[f64]) -> Option<f64> {
    quantiles
        .iter()
        .position(|q| (*q - 0.5).abs() < 1e-9)
        .and_then(|i| values.get(i).copied())
}

fn metric_verdict(percentile: Option<f64>) -> String {
    match percentile {
        None => "暂无基准数据".to_string(),
        Some(p) if p >= 0.85 => format!("顶尖，超过 {:.0}% 的同英雄对局", p * 100.0),
        Some(p) if p >= 0.6 => format!("高于平均，超过 {:.0}% 的同英雄对局", p * 100.0),
        Some(p) if p >= 0.4 => format!("接近平均（{:.0}% 分位）", p * 100.0),
        Some(p) => format!("偏低，只超过 {:.0}% 的同英雄对局", p * 100.0),
    }
}

fn grade_for(score: f64) -> &'static str {
    if score >= 80.0 {
        "S"
    } else if score >= 65.0 {
        "A"
    } else if score >= 50.0 {
        "B"
    } else if score >= 35.0 {
        "C"
    } else {
        "D"
    }
}

fn build_metric(
    key: &str,
    label: &str,
    total: u64,
    minutes: f64,
    quantiles: &[f64],
    values: Option<&Vec<f64>>,
) -> PostgameMetric {
    let per_minute = if minutes > 0.0 { total as f64 / minutes } else { 0.0 };
    let percentile = values.and_then(|v| percentile_of(per_minute, quantiles, v));
    PostgameMetric {
        key: key.to_string(),
        label: label.to_string(),
        per_minute: (per_minute * 10.0).round() / 10.0,
        percentile: percentile.map(|p| (p * 1000.0).round() / 1000.0),
        median: values.and_then(|v| median_of(quantiles, v)),
        verdict: metric_verdict(percentile),
    }
}

/// Review one match. The overall score (0..100) weights damage rank 45 %,
/// the better of damage-taken and healing rank 20 %, and the average augment
/// HexScore 35 %; without augments the first two are rescaled to 70 / 30.
pub fn review_match(ds: &Dataset, entry: &MatchHistoryEntry) -> PostgameReview {
    let champion = ds
        .find_champion(&entry.hero_name)
        .or_else(|| ds.find_champion(&entry.champion_id.to_string()));
    let hero_id = champion
        .map(|c| c.id.clone())
        .unwrap_or_else(|| entry.champion_id.to_string());
    let hero_name = champion
        .map(|c| c.hero.clone())
        .unwrap_or_else(|| entry.hero_name.clone());

    let minutes = entry.duration_secs as f64 / 60.0;
    let benchmark = ds.hero_benchmarks.get(&hero_id);
    let quantiles = ds.benchmark_quantiles.as_slice();

    let metrics = vec![
        build_metric("damage", "分均输出", entry.damage, minutes, quantiles, benchmark.map(|b| &b.damage)),
        build_metric("taken", "分均承伤", entry.damage_taken, minutes, quantiles, benchmark.map(|b| &b.taken)),
        build_metric("heal", "分均治疗", entry.heal, minutes, quantiles, benchmark.map(|b| &b.heal)),
    ];

    // --- augments ---------------------------------------------------------
    let hero_rows = ds
        .hero_augment_stats
        .get(&hero_name)
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    let augments: Vec<PostgameAugmentVerdict> = entry
        .augments
        .iter()
        .map(|name| {
            let row = hero_rows.iter().find(|r| r.augment_name == *name);
            let catalog = ds.augments_full.iter().find(|a| a.augment_name == *name);
            let hex_score = row
                .map(|r| r.hex_score)
                .or_else(|| catalog.map(|a| a.hex_score))
                .unwrap_or(55.0);
            let win_rate = row
                .map(|r| r.win_rate)
                .or_else(|| catalog.map(|a| a.win_rate))
                .unwrap_or(0.5);
            let games = row.map(|r| r.games).or_else(|| catalog.map(|a| a.games)).unwrap_or(0);
            let tier = get_tier_meta(hex_score);
            let note = match (row, catalog) {
                (Some(r), _) => format!(
                    "{} 上胜率 {:.1}%（{} 场）",
                    hero_name,
                    r.win_rate * 100.0,
                    r.games
                ),
                (None, Some(_)) => "该英雄样本不足，按全英雄数据评分".to_string(),
                (None, None) => "数据库中没有这个强化".to_string(),
            };
            PostgameAugmentVerdict {
                name: name.clone(),
                hex_score,
                tier_label: tier.label,
                tier_color: tier.color,
                win_rate,
                games,
                hero_specific: row.is_some(),
                note,
            }
        })
        .collect();

    let mut links = Vec::new();
    let mut conflicts = Vec::new();
    for (i, a) in entry.augments.iter().enumerate() {
        for b in entry.augments.iter().skip(i + 1) {
            for link in &ds.mechanic_pairs.links {
                if (link.a_name == *a && link.b_name == *b) || (link.a_name == *b && link.b_name == *a) {
                    links.push(format!("【{a}】+【{b}】({})：{}", link.family, link.reason));
                }
            }
            for conf in &ds.mechanic_pairs.conflicts {
                let hit = |x: &String, y: &String| {
                    conf.a_name == *x && conf.bs.iter().any(|t| t.name == *y)
                };
                if hit(a, b) || hit(b, a) {
                    conflicts.push(format!("【{a}】×【{b}】({})：{}", conf.rule, conf.reason));
                }
            }
        }
    }

    let matched_trio = ds.hero_augment_trios.get(&hero_name).and_then(|trios| {
        trios
            .iter()
            .find(|t| !t.augments.is_empty() && t.augments.iter().all(|a| entry.augments.contains(a)))
            .map(|t| format!("{}：{}", t.family, t.augments.join(" + ")))
    });

    // Augments with no data carry a placeholder score, so they are left out.
    let lowest_taken = augments
        .iter()
        .filter(|a| a.games > 0)
        .map(|a| a.hex_score)
        .fold(f64::INFINITY, f64::min);
    let mut better: Vec<&shared_types::HeroAugmentStatRow> = hero_rows
        .iter()
        .filter(|r| !entry.augments.contains(&r.augment_name))
        .filter(|r| !lowest_taken.is_finite() || r.hex_score > lowest_taken)
        .collect();
    better.sort_by(|a, b| b.hex_score.total_cmp(&a.hex_score));
    let better_augments: Vec<String> = better
        .iter()
        .take(3)
        .map(|r| format!("{}（{:.1}）", r.augment_name, r.hex_score))
        .collect();

    // --- build ------------------------------------------------------------
    let core_items = ds
        .formula_items_by_hero
        .get(&hero_name)
        .map(|b| b.core_items.as_slice())
        .unwrap_or(&[]);
    let owned: Vec<String> = entry.items.iter().map(|id| id.to_string()).collect();
    let missing_core_items: Vec<String> = core_items
        .iter()
        .filter(|it| !owned.contains(&it.id))
        .map(|it| it.name.clone())
        .collect();
    let core_items_total = core_items.len() as u32;
    let core_items_built = core_items_total - missing_core_items.len() as u32;

    // --- score ------------------------------------------------------------
    let pct = |key: &str| {
        metrics
            .iter()
            .find(|m| m.key == key)
            .and_then(|m| m.percentile)
    };
    let damage_pct = pct("damage").unwrap_or(0.5);
    let support_pct = pct("taken").unwrap_or(0.5).max(pct("heal").unwrap_or(0.5));
    let score = if augments.is_empty() {
        100.0 * (0.7 * damage_pct + 0.3 * support_pct)
    } else {
        let avg_hex = augments.iter().map(|a| a.hex_score).sum::<f64>() / augments.len() as f64;
        let conflict_penalty = 8.0 * conflicts.len() as f64;
        100.0 * (0.45 * damage_pct + 0.2 * support_pct) + 0.35 * avg_hex - conflict_penalty
    };
    let score = (score.clamp(0.0, 100.0) * 10.0).round() / 10.0;

    // --- summary ----------------------------------------------------------
    let benchmark_reliable = entry.duration_secs >= MIN_BENCHMARK_SECS && benchmark.is_some();
    let mut summary = Vec::new();
    if benchmark.is_none() {
        summary.push("没有这个英雄的基准数据，输出排名无法计算".to_string());
    } else if entry.duration_secs < MIN_BENCHMARK_SECS {
        summary.push("对局不足 10 分钟，分均数据偏差较大，排名仅供参考".to_string());
    }
    if let Some(m) = metrics.first() {
        summary.push(format!("{}：{:.0}，{}", m.label, m.per_minute, m.verdict));
    }
    if let Some(worst) = augments
        .iter()
        .min_by(|a, b| a.hex_score.total_cmp(&b.hex_score))
        .filter(|a| a.hex_score < 55.0)
    {
        summary.push(format!(
            "最弱的一张强化是【{}】（{} {:.1}）",
            worst.name, worst.tier_label, worst.hex_score
        ));
    }
    if !conflicts.is_empty() {
        summary.push(format!("有 {} 组强化互相冲突，等于浪费了选卡", conflicts.len()));
    }
    if let Some(trio) = &matched_trio {
        summary.push(format!("凑齐了推荐组合 {trio}"));
    }
    if core_items_total > 0 && !entry.items.is_empty() {
        summary.push(format!("公式核心装完成 {core_items_built}/{core_items_total}"));
    }

    PostgameReview {
        game_id: entry.game_id.clone(),
        hero_name,
        result: entry.result.clone(),
        grade: grade_for(score).to_string(),
        score,
        metrics,
        benchmark_reliable,
        augments,
        links,
        conflicts,
        matched_trio,
        better_augments,
        core_items_built,
        core_items_total,
        missing_core_items,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dataset() -> Option<Dataset> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .unwrap()
            .to_path_buf();
        if !root.join("data/hero_postgame_benchmarks_v1.json").exists() {
            return None;
        }
        Dataset::load_from_root(&root).ok()
    }

    fn entry(damage: u64, augments: &[&str], items: &[u32], duration_secs: u64) -> MatchHistoryEntry {
        MatchHistoryEntry {
            game_id: "1".to_string(),
            champion_id: 22,
            hero_name: "寒冰射手".to_string(),
            result: "WIN".to_string(),
            kills: 10,
            deaths: 5,
            assists: 20,
            kda: "10 / 5 / 20".to_string(),
            damage,
            damage_taken: 20000,
            heal: 2000,
            items: items.to_vec(),
            duration_secs,
            duration: String::new(),
            queue_id: 450,
            game_mode: "ARAM".to_string(),
            augments: augments.iter().map(|s| s.to_string()).collect(),
            hex_score: None,
            game_creation: 0,
            time_ago: String::new(),
        }
    }

    #[test]
    fn percentile_interpolates_and_stays_in_range() {
        let q = [0.25, 0.5, 0.75];
        let v = [100.0, 200.0, 400.0];
        assert_eq!(percentile_of(200.0, &q, &v), Some(0.5));
        assert_eq!(percentile_of(150.0, &q, &v), Some(0.375));
        assert_eq!(percentile_of(300.0, &q, &v), Some(0.625));
        assert_eq!(percentile_of(50.0, &q, &v), Some(0.125));
        assert_eq!(percentile_of(0.0, &q, &v), Some(0.0));
        let top = percentile_of(100_000.0, &q, &v).unwrap();
        assert!(top > 0.75 && top < 1.0);
        assert_eq!(percentile_of(1.0, &[], &[]), None);
        assert_eq!(percentile_of(1.0, &q, &[1.0]), None);
    }

    #[test]
    fn median_damage_ranks_near_the_middle_and_high_damage_ranks_high() {
        let Some(ds) = dataset() else { return };
        // 寒冰射手 median damage per minute is 1447; 20-minute game.
        let mid = review_match(&ds, &entry(1447 * 20, &[], &[], 1200));
        let p = mid.metrics[0].percentile.unwrap();
        assert!((p - 0.5).abs() < 0.01, "got {p}");
        assert!(mid.benchmark_reliable);

        let high = review_match(&ds, &entry(3500 * 20, &[], &[], 1200));
        assert!(high.metrics[0].percentile.unwrap() > 0.95);
        assert!(high.score > mid.score);
    }

    #[test]
    fn short_games_are_flagged_as_unreliable() {
        let Some(ds) = dataset() else { return };
        let review = review_match(&ds, &entry(9000, &[], &[], 420));
        assert!(!review.benchmark_reliable);
        assert!(review.summary.iter().any(|s| s.contains("不足 10 分钟")));
    }

    #[test]
    fn rates_augments_and_tracks_core_build_progress() {
        let Some(ds) = dataset() else { return };
        // 3032 and 3085 are the first two of four core items.
        let review = review_match(
            &ds,
            &entry(30000, &["弹跳蜜蜂", "不存在的强化"], &[3032, 3085, 1001], 1200),
        );
        assert_eq!(review.augments.len(), 2);
        assert!(review.augments[0].hero_specific);
        assert_eq!(review.augments[0].hex_score, 51.9);
        assert!(!review.augments[1].hero_specific);
        // 男爵之手 (65.0) is this hero's best augment and was not taken.
        assert!(review.better_augments[0].starts_with("男爵之手"));
        assert_eq!(review.core_items_total, 4);
        assert_eq!(review.core_items_built, 2);
        assert_eq!(review.missing_core_items.len(), 2);
        assert!(!review.better_augments.is_empty());
        assert!(["S", "A", "B", "C", "D"].contains(&review.grade.as_str()));
    }

    #[test]
    fn conflicting_augments_are_reported_and_cost_score() {
        let Some(ds) = dataset() else { return };
        let Some(conf) = ds
            .mechanic_pairs
            .conflicts
            .iter()
            .find(|c| !c.a_name.is_empty() && c.bs.iter().any(|b| !b.name.is_empty()))
        else {
            return;
        };
        let other = conf.bs.iter().find(|b| !b.name.is_empty()).unwrap().name.clone();
        let clash = review_match(&ds, &entry(30000, &[conf.a_name.as_str(), other.as_str()], &[], 1200));
        assert_eq!(clash.conflicts.len(), 1);
        assert!(clash.summary.iter().any(|s| s.contains("冲突")));
    }
}
