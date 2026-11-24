use data_runtime::Dataset;
use shared_types::{EnemyCounterAnalysis, EnemyCounterItemSuggestion};

/// Analyzes enemy team composition to recommend dynamic counter items.
/// Algorithm matches `dataService.ts:analyzeEnemyCounter`.
pub fn analyze_enemy_counter(
    ds: &Dataset,
    ally_hero_name: &str,
    enemy_hero_names: &[String],
) -> EnemyCounterAnalysis {
    if enemy_hero_names.is_empty() {
        return EnemyCounterAnalysis {
            state: "waiting".to_string(),
            lean: "balanced".to_string(),
            physical_share: 0.5,
            magic_share: 0.45,
            true_share: 0.05,
            boots: None,
            anti_heal: None,
            defensive_items: Vec::new(),
            healers: Vec::new(),
        };
    }

    let mut phys_w = 0.0_f64;
    let mut magic_w = 0.0_f64;
    let mut total_dmg = 0.0_f64;
    let mut total_heal = 0.0_f64;
    let mut total_taken = 0.0_f64;
    let mut total_cc = 0.0_f64;
    let mut healers = Vec::new();

    for ename in enemy_hero_names {
        let ch = ds.find_champion(ename);
        let pm = ch.and_then(|c| ds.postmatch_by_hero_id.get(&c.id));

        let dmg = pm
            .and_then(|v| v.get("avgDamage").and_then(|x| x.as_f64()))
            .or_else(|| ch.and_then(|c| c.avg_damage))
            .unwrap_or(32000.0);
        let heal = pm
            .and_then(|v| v.get("avgHealShield").and_then(|x| x.as_f64()))
            .or_else(|| ch.and_then(|c| c.avg_heal_shield))
            .unwrap_or(4500.0);
        let taken = pm
            .and_then(|v| v.get("avgDamageTaken").and_then(|x| x.as_f64()))
            .or_else(|| ch.and_then(|c| c.avg_damage_taken))
            .unwrap_or(28000.0);
        let cc = pm
            .and_then(|v| v.get("avgCcTime").and_then(|x| x.as_f64()))
            .or_else(|| ch.and_then(|c| c.avg_cc_time))
            .unwrap_or(25.0);

        let roles = ch.map(|c| c.roles.as_slice()).unwrap_or(&[]);
        let has_mage = roles.iter().any(|r| r == "法师");
        let has_adc = roles.iter().any(|r| r == "射手");

        if has_mage && !has_adc {
            magic_w += dmg * 0.8;
            phys_w += dmg * 0.14;
        } else if has_adc {
            phys_w += dmg * 0.84;
            magic_w += dmg * 0.1;
        } else {
            phys_w += dmg * 0.55;
            magic_w += dmg * 0.38;
        }

        total_dmg += dmg;
        total_heal += heal;
        total_taken += taken;
        total_cc += cc;
        if heal >= 9200.0 {
            healers.push(ch.map(|c| c.hero.clone()).unwrap_or_else(|| ename.clone()));
        }
    }

    let denom = total_dmg.max(1.0);
    let physical_share = (phys_w / denom).clamp(0.08, 0.9);
    let magic_share = (magic_w / denom).clamp(0.08, 0.9);
    let true_share = (1.0 - physical_share - magic_share).max(0.02);

    let lean = if physical_share >= 0.64 {
        "physical"
    } else if magic_share >= 0.64 {
        "magic"
    } else {
        "balanced"
    };

    let ap_ids = ["3089", "4645", "6655", "6653", "3157", "3118"];
    let is_ap = ds
        .formula_items_by_hero
        .get(ally_hero_name)
        .map(|f| f.core_items.iter().any(|it| ap_ids.contains(&it.id.as_str())))
        .unwrap_or(false);

    let enemy_cnt = enemy_hero_names.len() as f64;
    let boots = if lean == "physical" {
        Some(EnemyCounterItemSuggestion {
            item_id: 3047,
            item_name: "铁板靴".to_string(),
            reason: format!(
                "敌方物理伤害占比高达 {:.0}%，护甲鞋减免普攻",
                physical_share * 100.0
            ),
            lift: 0.029,
        })
    } else if lean == "magic" || total_cc / enemy_cnt >= 36.0 {
        Some(EnemyCounterItemSuggestion {
            item_id: 3111,
            item_name: "水银之靴".to_string(),
            reason: format!(
                "敌方魔法伤害 {:.0}% 且场均控制 {:.0}s",
                magic_share * 100.0,
                total_cc / enemy_cnt
            ),
            lift: 0.032,
        })
    } else {
        None
    };

    let healer_summary = if healers.is_empty() {
        "团队高治疗".to_string()
    } else {
        healers.join(" / ")
    };

    let anti_heal = if healers.len() >= 2 || total_heal / enemy_cnt >= 8000.0 {
        if is_ap {
            Some(EnemyCounterItemSuggestion {
                item_id: 3165,
                item_name: "莫雷洛秘典".to_string(),
                reason: format!("敌方高回复阵容 ({healer_summary})，重伤必出"),
                lift: 0.037,
            })
        } else {
            Some(EnemyCounterItemSuggestion {
                item_id: 3033,
                item_name: "凡性的提醒".to_string(),
                reason: format!("重伤 + 百分比穿甲克制敌方回复 ({healer_summary})"),
                lift: 0.041,
            })
        }
    } else {
        None
    };

    let mut defensive_items = Vec::new();
    if total_taken / enemy_cnt >= 34000.0 {
        if is_ap {
            defensive_items.push(EnemyCounterItemSuggestion {
                item_id: 6653,
                item_name: "兰德里的折磨".to_string(),
                reason: "敌方前排血量与坦度极高，持续百分比灼烧收益显著".to_string(),
                lift: 0.035,
            });
        } else {
            defensive_items.push(EnemyCounterItemSuggestion {
                item_id: 3036,
                item_name: "多米尼克领主的致意".to_string(),
                reason: "敌方前排护甲深厚，高额百分比护甲穿透必出".to_string(),
                lift: 0.043,
            });
        }
    }

    if lean == "physical" {
        defensive_items.push(if is_ap {
            EnemyCounterItemSuggestion {
                item_id: 3157,
                item_name: "中娅沙漏".to_string(),
                reason: "规避敌方高物理刺客与集火爆发".to_string(),
                lift: 0.03,
            }
        } else {
            EnemyCounterItemSuggestion {
                item_id: 6333,
                item_name: "死亡之舞".to_string(),
                reason: "延迟物理爆发伤害并提供击杀回血".to_string(),
                lift: 0.031,
            }
        });
    } else if lean == "magic" {
        defensive_items.push(if is_ap {
            EnemyCounterItemSuggestion {
                item_id: 3102,
                item_name: "女妖面纱".to_string(),
                reason: "法术护盾抵挡敌方先手开团与高额 Poke".to_string(),
                lift: 0.028,
            }
        } else {
            EnemyCounterItemSuggestion {
                item_id: 3156,
                item_name: "玛莫提乌斯之噬".to_string(),
                reason: "高额魔法护盾应对敌方多 AP 爆发".to_string(),
                lift: 0.029,
            }
        });
    }

    EnemyCounterAnalysis {
        state: "ready".to_string(),
        lean: lean.to_string(),
        physical_share,
        magic_share,
        true_share,
        boots,
        anti_heal,
        defensive_items,
        healers,
    }
}
