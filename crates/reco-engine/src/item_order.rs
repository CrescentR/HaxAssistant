//! Item purchase order.
//!
//! Turns a hero's formula build plus the enemy-specific counter items into an
//! ordered shopping plan, credits components already in the inventory, and
//! says what the current gold can buy right now.

use std::collections::HashMap;

use data_runtime::Dataset;
use shared_types::{ItemPlan, ItemPlanStep, ItemPrice, ItemPurchase};

use crate::conditional_items::analyze_enemy_counter;

const BOOTS_ID: &str = "1001";

/// Inventory as a multiset of item ids.
type Inventory = HashMap<String, u32>;

fn inventory_from(owned_item_ids: &[u32]) -> Inventory {
    let mut inv = Inventory::new();
    for id in owned_item_ids {
        *inv.entry(id.to_string()).or_insert(0) += 1;
    }
    inv
}

fn take(inv: &mut Inventory, item_id: &str) -> bool {
    match inv.get_mut(item_id) {
        Some(count) if *count > 0 => {
            *count -= 1;
            true
        }
        _ => false,
    }
}

/// Gold needed to finish `item_id`, consuming matching components from `inv`.
/// Items missing from the price table cost 0 so they never block the plan.
fn remaining_cost(prices: &HashMap<String, ItemPrice>, inv: &mut Inventory, item_id: &str) -> u32 {
    if take(inv, item_id) {
        return 0;
    }
    let Some(price) = prices.get(item_id) else { return 0 };
    if price.from.is_empty() {
        return price.total;
    }
    price.base
        + price
            .from
            .iter()
            .map(|component| remaining_cost(prices, inv, component))
            .sum::<u32>()
}

/// True if `item_id` is, or is built from, `component_id`.
fn builds_from(prices: &HashMap<String, ItemPrice>, item_id: &str, component_id: &str) -> bool {
    if item_id == component_id {
        return true;
    }
    prices
        .get(item_id)
        .is_some_and(|p| p.from.iter().any(|c| builds_from(prices, c, component_id)))
}

/// Missing components of `item_id`, descending into any that are not owned.
/// Each entry is (component id, gold needed to get that component).
fn missing_components(
    prices: &HashMap<String, ItemPrice>,
    inv: &mut Inventory,
    item_id: &str,
    out: &mut Vec<(String, u32)>,
) {
    let Some(price) = prices.get(item_id) else { return };
    for component in &price.from {
        if take(inv, component) {
            continue;
        }
        let mut probe = inv.clone();
        let cost = remaining_cost(prices, &mut probe, component);
        out.push((component.clone(), cost));
    }
}

/// Greedy partial purchase toward `item_id`: the most expensive missing
/// component that fits, repeated; components too dear are broken down further.
fn buy_components(
    prices: &HashMap<String, ItemPrice>,
    inv: &mut Inventory,
    item_id: &str,
    gold: &mut u32,
    out: &mut Vec<ItemPurchase>,
) {
    let mut missing = Vec::new();
    let mut probe = inv.clone();
    missing_components(prices, &mut probe, item_id, &mut missing);
    missing.sort_by(|a, b| b.1.cmp(&a.1));

    for (component, cost) in missing {
        if cost == 0 {
            continue;
        }
        if cost <= *gold {
            *gold -= cost;
            // The sub-components it absorbs leave the inventory with it.
            let _ = remaining_cost(prices, inv, &component);
            out.push(ItemPurchase {
                item_name: item_name(prices, &component),
                item_id: component,
                cost,
            });
        } else {
            buy_components(prices, inv, &component, gold, out);
        }
    }
}

fn item_name(prices: &HashMap<String, ItemPrice>, item_id: &str) -> String {
    prices
        .get(item_id)
        .map(|p| p.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("装备 #{item_id}"))
}

struct Planned {
    id: String,
    name: String,
    kind: &'static str,
    reason: String,
}

/// Build order before inventory is considered: starters, then core items with
/// counter items slotted in (boots after the 1st core item, anti-heal after
/// the 2nd, the top defensive item after the 3rd).
fn planned_sequence(ds: &Dataset, hero_name: &str, enemy_names: &[String]) -> Vec<Planned> {
    let mut seq: Vec<Planned> = Vec::new();
    let Some(build) = ds.formula_items_by_hero.get(hero_name) else {
        return seq;
    };

    for it in &build.starter_items {
        seq.push(Planned {
            id: it.id.clone(),
            name: it.name.clone(),
            kind: "starter",
            reason: "出门装".to_string(),
        });
    }

    let counter = analyze_enemy_counter(ds, hero_name, enemy_names);
    let mut after_core: HashMap<usize, Planned> = HashMap::new();
    let mut slot = |position: usize, suggestion: Option<&shared_types::EnemyCounterItemSuggestion>| {
        if let Some(s) = suggestion {
            after_core.insert(
                position,
                Planned {
                    id: s.item_id.to_string(),
                    name: s.item_name.clone(),
                    kind: "counter",
                    reason: s.reason.clone(),
                },
            );
        }
    };
    slot(1, counter.boots.as_ref());
    slot(2, counter.anti_heal.as_ref());
    slot(3, counter.defensive_items.first());

    for (index, it) in build.core_items.iter().enumerate() {
        seq.push(Planned {
            id: it.id.clone(),
            name: it.name.clone(),
            kind: "core",
            reason: format!("公式核心第 {} 件", index + 1),
        });
        if let Some(extra) = after_core.remove(&(index + 1)) {
            let duplicate = seq.iter().any(|p| p.id == extra.id);
            if !duplicate {
                seq.push(extra);
            }
        }
    }
    seq
}

/// Plan the purchase order for `hero_name`.
///
/// * `owned_item_ids` - the player's current inventory.
/// * `enemy_names` - enemy champions, used to pick counter items.
pub fn plan_item_order(
    ds: &Dataset,
    hero_name: &str,
    current_gold: f64,
    owned_item_ids: &[u32],
    enemy_names: &[String],
) -> ItemPlan {
    let prices = &ds.item_prices;
    let gold = current_gold.max(0.0).floor() as u32;
    let mut plan = ItemPlan {
        hero_name: hero_name.to_string(),
        current_gold: gold,
        gold_after: gold,
        ..ItemPlan::default()
    };

    let sequence = planned_sequence(ds, hero_name, enemy_names);
    if sequence.is_empty() {
        plan.note = "暂无该英雄的公式出装数据".to_string();
        return plan;
    }

    let owned_ids: Vec<String> = owned_item_ids.iter().map(|id| id.to_string()).collect();
    let owns_boots = owned_ids.iter().any(|id| builds_from(prices, id, BOOTS_ID));
    // Starters are skipped once the inventory is worth more than they are:
    // by then they have been bought or built into something else.
    let inventory_value: u32 = owned_ids
        .iter()
        .filter_map(|id| prices.get(id))
        .map(|p| p.total)
        .sum();
    let starter_value: u32 = sequence
        .iter()
        .filter(|p| p.kind == "starter")
        .filter_map(|p| prices.get(&p.id))
        .map(|p| p.total)
        .sum();
    let past_starters = inventory_value > 0 && inventory_value >= starter_value;

    let mut inv = inventory_from(owned_item_ids);
    for planned in &sequence {
        let total_cost = prices.get(&planned.id).map(|p| p.total).unwrap_or(0);
        let is_boots_upgrade = planned.kind == "counter" && builds_from(prices, &planned.id, BOOTS_ID);

        let (owned, remaining) = if planned.kind == "starter" && past_starters {
            let _ = take(&mut inv, &planned.id);
            (true, 0)
        } else if is_boots_upgrade
            && owns_boots
            && !owned_ids.contains(&planned.id)
            && !owned_ids.iter().any(|id| id == BOOTS_ID)
        {
            // Already wearing a different upgraded pair: don't ask for a second.
            (true, 0)
        } else {
            let cost = remaining_cost(prices, &mut inv, &planned.id);
            (cost == 0 && total_cost > 0, cost)
        };

        plan.steps.push(ItemPlanStep {
            item_id: planned.id.clone(),
            item_name: planned.name.clone(),
            kind: planned.kind.to_string(),
            total_cost,
            remaining_cost: remaining,
            owned,
            reason: planned.reason.clone(),
        });
    }

    // Spend the gold in build order; partial progress goes into components.
    // `remaining_cost` already credited what is owned, so a fresh inventory is
    // replayed for the component breakdown.
    let mut gold_left = gold;
    let mut shop_inv = inventory_from(owned_item_ids);
    let mut first_unowned: Option<ItemPlanStep> = None;
    let mut blocked_on: Option<(ItemPlanStep, u32)> = None;

    for step in &plan.steps {
        if step.owned {
            let _ = remaining_cost(prices, &mut shop_inv, &step.item_id);
            continue;
        }
        if first_unowned.is_none() {
            first_unowned = Some(step.clone());
        }
        if step.remaining_cost <= gold_left {
            gold_left -= step.remaining_cost;
            let _ = remaining_cost(prices, &mut shop_inv, &step.item_id);
            plan.buy_now.push(ItemPurchase {
                item_id: step.item_id.clone(),
                item_name: step.item_name.clone(),
                cost: step.remaining_cost,
            });
        } else {
            let before = gold_left;
            buy_components(prices, &mut shop_inv, &step.item_id, &mut gold_left, &mut plan.buy_now);
            let spent = before - gold_left;
            blocked_on = Some((step.clone(), step.remaining_cost - spent));
            break;
        }
    }

    plan.gold_after = gold_left;
    plan.next_item = first_unowned;
    plan.note = match (&plan.next_item, &blocked_on) {
        (None, _) => "出装已成型，剩余金币可补药水或换装".to_string(),
        (Some(_), Some((step, short))) => {
            plan.gold_short = short.saturating_sub(gold_left);
            if plan.buy_now.is_empty() {
                format!("攒钱：距离【{}】还差 {} 金币", step.item_name, plan.gold_short)
            } else {
                format!(
                    "先买散件，距离【{}】还差 {} 金币",
                    step.item_name, plan.gold_short
                )
            }
        }
        (Some(_), None) => "金币足够买完整套出装".to_string(),
    };
    plan
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
        if !root.join("data/item_prices_v1.json").exists() {
            return None;
        }
        Dataset::load_from_root(&root).ok()
    }

    fn price_table() -> HashMap<String, ItemPrice> {
        // 9000 (3000) = 400 recipe + two 9001 (900 each) + 9002 (800)
        // 9001 (900)  = 500 recipe + 9003 (400)
        let mk = |name: &str, total, base, from: &[&str]| ItemPrice {
            name: name.to_string(),
            total,
            base,
            from: from.iter().map(|s| s.to_string()).collect(),
        };
        HashMap::from([
            ("9000".to_string(), mk("大件", 3000, 400, &["9001", "9002", "9001"])),
            ("9001".to_string(), mk("腰带", 900, 500, &["9003"])),
            ("9002".to_string(), mk("护符", 800, 800, &[])),
            ("9003".to_string(), mk("水晶", 400, 400, &[])),
        ])
    }

    #[test]
    fn remaining_cost_credits_owned_components_once() {
        let prices = price_table();
        assert_eq!(remaining_cost(&prices, &mut Inventory::new(), "9000"), 3000);
        // One belt owned: only one of the two belt slots is credited.
        assert_eq!(remaining_cost(&prices, &mut inventory_from(&[9001]), "9000"), 2100);
        assert_eq!(
            remaining_cost(&prices, &mut inventory_from(&[9001, 9001, 9002]), "9000"),
            400
        );
        // A crystal counts toward a belt that is not built yet.
        assert_eq!(remaining_cost(&prices, &mut inventory_from(&[9003]), "9000"), 2600);
        assert_eq!(remaining_cost(&prices, &mut inventory_from(&[9000]), "9000"), 0);
    }

    #[test]
    fn partial_gold_buys_the_dearest_components_that_fit() {
        let prices = price_table();
        let mut gold = 1400;
        let mut bought = Vec::new();
        buy_components(&prices, &mut Inventory::new(), "9000", &mut gold, &mut bought);
        let ids: Vec<&str> = bought.iter().map(|b| b.item_id.as_str()).collect();
        // Belt (900) first, then a crystal (400) toward the second belt.
        assert_eq!(ids, vec!["9001", "9003"]);
        assert_eq!(gold, 100);
    }

    #[test]
    fn plans_full_build_for_a_real_hero() {
        let Some(ds) = dataset() else { return };
        let plan = plan_item_order(&ds, "寒冰射手", 1400.0, &[], &[]);
        assert!(plan.steps.iter().any(|s| s.kind == "starter"));
        assert_eq!(plan.steps.iter().filter(|s| s.kind == "core").count(), 4);
        assert_eq!(plan.next_item.as_ref().unwrap().kind, "starter");
        let spent: u32 = plan.buy_now.iter().map(|b| b.cost).sum();
        assert_eq!(spent + plan.gold_after, 1400);
        assert!(!plan.buy_now.is_empty());
    }

    #[test]
    fn owned_items_are_skipped_and_counter_items_are_inserted() {
        let Some(ds) = dataset() else { return };
        let enemies: Vec<String> = ["疾风剑豪", "暗裔剑魔", "祖安狂人", "影流之主", "无双剑姬"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // Owns the first core item (育恩塔尔荒野箭 3032).
        let plan = plan_item_order(&ds, "寒冰射手", 300.0, &[3032], &enemies);
        let first_core = plan.steps.iter().find(|s| s.kind == "core").unwrap();
        assert!(first_core.owned);
        assert!(plan.steps.iter().filter(|s| s.kind == "starter").all(|s| s.owned));
        assert!(plan.steps.iter().any(|s| s.kind == "counter"));
        let next = plan.next_item.as_ref().unwrap();
        assert!(!next.owned);
        assert!(plan.gold_short > 0);
        assert!(plan.note.contains(&next.item_name));
    }

    #[test]
    fn unknown_hero_returns_an_empty_plan_with_a_note() {
        let Some(ds) = dataset() else { return };
        let plan = plan_item_order(&ds, "不存在的英雄", 5000.0, &[], &[]);
        assert!(plan.steps.is_empty());
        assert!(!plan.note.is_empty());
    }
}
