pub mod conditional_items;
pub mod item_order;
pub mod lineup;
pub mod postgame;

use data_runtime::Dataset;
use shared_types::{get_tier_meta, AugmentRecommendationCard};

pub use conditional_items::analyze_enemy_counter;
pub use item_order::plan_item_order;
pub use lineup::evaluate_5v5_lineup;
pub use postgame::review_match;

/// Evaluates 3 candidate Hextech Augment cards for a given hero, stage (1..4),
/// and already confirmed augments. Algorithm matches `dataService.ts:evaluateAugmentOffer`.
pub fn evaluate_augment_offer(
    ds: &Dataset,
    hero_name: &str,
    candidate_names: &[String],
    stage: u8,
    confirmed_augments: &[String],
) -> Vec<AugmentRecommendationCard> {
    let hero_stats = ds
        .hero_augment_stats
        .get(hero_name)
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    let mut cards: Vec<AugmentRecommendationCard> = candidate_names
        .iter()
        .enumerate()
        .map(|(idx, cand_name)| {
            let row = hero_stats.iter().find(|r| r.augment_name == *cand_name);
            let cat = ds
                .augments_full
                .iter()
                .find(|a| a.augment_name == *cand_name);

            let base_hex = row
                .map(|r| r.hex_score)
                .or_else(|| cat.map(|a| a.hex_score))
                .unwrap_or(56.0);
            let win_rate = row
                .map(|r| r.win_rate)
                .or_else(|| cat.map(|a| a.win_rate))
                .unwrap_or(0.5);
            let pick_rate = row
                .map(|r| r.pick_rate)
                .or_else(|| cat.map(|a| a.pick_rate))
                .unwrap_or(0.05);
            let delta_wr = row
                .map(|r| r.delta_win_rate)
                .unwrap_or(win_rate - 0.5);
            let sample_size = row
                .map(|r| r.games)
                .or_else(|| cat.map(|a| a.games))
                .unwrap_or(1200);
            let rarity = row
                .map(|r| r.rarity.clone())
                .or_else(|| cat.map(|a| a.rarity.clone()))
                .unwrap_or_else(|| "黄金".to_string());
            let stage_shift = ds
                .stage_shifts
                .get(cand_name)
                .and_then(|m| m.get(&stage))
                .copied()
                .unwrap_or(0.0);

            let mut synergy_note: Option<String> = None;
            let mut conflict_note: Option<String> = None;
            let mut synergy_bonus = 0.0_f64;
            let mut has_conflict = false;

            for chosen in confirmed_augments {
                for link in &ds.mechanic_pairs.links {
                    if (link.a_name == *cand_name && link.b_name == *chosen)
                        || (link.b_name == *cand_name && link.a_name == *chosen)
                    {
                        synergy_bonus += 8.5;
                        synergy_note = Some(format!(
                            "机制联动【{}】({})：{}",
                            chosen, link.family, link.reason
                        ));
                    }
                }
                for conf in &ds.mechanic_pairs.conflicts {
                    let chosen_is_a = conf.a_name == *chosen;
                    let cand_is_a = conf.a_name == *cand_name;
                    let chosen_in_b = conf.bs.iter().any(|b| b.name == *chosen);
                    let cand_in_b = conf.bs.iter().any(|b| b.name == *cand_name);
                    if (chosen_is_a && cand_in_b) || (cand_is_a && chosen_in_b) {
                        has_conflict = true;
                        conflict_note = Some(format!(
                            "废卡冲突【{}】({})：{}",
                            chosen, conf.rule, conf.reason
                        ));
                    }
                }
            }

            let mut final_hex =
                (base_hex + stage_shift * 180.0 + synergy_bonus).clamp(5.0, 100.0);
            if has_conflict {
                final_hex = 12.0;
            }
            final_hex = (final_hex * 10.0).round() / 10.0;
            let tier_meta = get_tier_meta(final_hex);

            let stage_tag = if stage_shift.abs() >= 0.01 {
                format!(
                    " · 第 {} 轮修正 {}{:.1}%",
                    stage,
                    if stage_shift >= 0.0 { "+" } else { "" },
                    stage_shift * 100.0
                )
            } else {
                String::new()
            };

            let augment_id = row
                .map(|r| r.augment_id)
                .or_else(|| cat.map(|a| a.augment_id))
                .unwrap_or(0);

            let top_items = row
                .map(|r| r.top_items.iter().take(4).cloned().collect())
                .unwrap_or_default();

            AugmentRecommendationCard {
                slot: idx + 1,
                hero_name: hero_name.to_string(),
                augment_id,
                augment_name: cand_name.clone(),
                rarity: rarity.clone(),
                hex_score: final_hex,
                hex_tier: tier_meta.tier,
                hex_label: tier_meta.label,
                hex_tier_color: tier_meta.color,
                win_rate,
                pick_rate,
                delta_win_rate: delta_wr,
                sample_size,
                stage_shift,
                reason: format!(
                    "{} · 基础 Hex {:.1} · 净胜率 {}{:.1}%{}",
                    rarity,
                    base_hex,
                    if delta_wr >= 0.0 { "+" } else { "" },
                    delta_wr * 100.0,
                    stage_tag
                ),
                synergy_note,
                conflict_note,
                top_items,
                is_best: false,
            }
        })
        .collect();

    if !cards.is_empty() {
        let mut best_idx = 0usize;
        for i in 1..cards.len() {
            if cards[i].hex_score > cards[best_idx].hex_score {
                best_idx = i;
            }
        }
        cards[best_idx].is_best = true;
    }

    cards
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn evaluates_augment_offer_and_conflict() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .unwrap()
            .to_path_buf();
        let ds = Dataset::load_from_root(&root).expect("load dataset");
        let cards = evaluate_augment_offer(
            &ds,
            "寒冰射手",
            &[
                "亮出你的剑".to_string(),
                "暴击飞弹".to_string(),
                "最终形态".to_string(),
            ],
            2,
            &["易损".to_string()],
        );
        assert_eq!(cards.len(), 3);
        assert_eq!(cards.iter().filter(|c| c.is_best).count(), 1);
    }

    #[test]
    fn evaluates_counter_and_5v5_lineup() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .unwrap()
            .to_path_buf();
        let ds = Dataset::load_from_root(&root).expect("load dataset");
        let enemies: Vec<String> = ["疾风剑豪", "暗裔剑魔", "祖安狂人", "众星之子", "影流之主"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let counter = analyze_enemy_counter(&ds, "寒冰射手", &enemies);
        assert_eq!(counter.state, "ready");
        if !ds.champions.is_empty() {
            assert!(counter.anti_heal.is_some());
        }

        let allies: Vec<String> = ["寒冰射手", "光辉女郎", "探险家", "皮城女警", "深海泰坦"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let bench: Vec<String> = ["曙光女神", "卡牌大师"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let lineup = evaluate_5v5_lineup(&ds, &allies, &enemies, &bench, "寒冰射手");
        assert!(lineup.adjusted_win > 0.1 && lineup.adjusted_win < 0.9);
        assert_eq!(lineup.bench_suggestions.len(), 2);
    }
}
