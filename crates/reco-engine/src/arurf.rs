//! ARURF (无限火力) recommendation & rune page planner, plus ARAM summoner spell advisor.

use data_runtime::Dataset;
use shared_types::{ArurfRecommendation, FormulaItemRef, RunePagePlan};

/// League of Legends summoner spell IDs.
pub const SPELL_CLEANSE: u32 = 1;
pub const SPELL_EXHAUST: u32 = 3;
pub const SPELL_FLASH: u32 = 4;
pub const SPELL_GHOST: u32 = 6;
pub const SPELL_HEAL: u32 = 7;
pub const SPELL_CLARITY: u32 = 13;
pub const SPELL_IGNITE: u32 = 14;
pub const SPELL_BARRIER: u32 = 21;
pub const SPELL_MARK: u32 = 32; // ARAM Snowball

pub fn summoner_spell_name(id: u32) -> &'static str {
    match id {
        SPELL_CLEANSE => "净化",
        SPELL_EXHAUST => "虚弱",
        SPELL_FLASH => "闪现",
        SPELL_GHOST => "幽灵疾步",
        SPELL_HEAL => "治疗术",
        SPELL_CLARITY => "清晰术",
        SPELL_IGNITE => "引燃",
        SPELL_BARRIER => "屏障",
        SPELL_MARK => "标记 (雪球)",
        _ => "未知技能",
    }
}

/// Recommend ARAM summoner spell pair (`(spell1_id, spell2_id)`) respecting user's Flash slot (`"D"` or `"F"`).
pub fn recommend_aram_spells(ds: &Dataset, hero_name: &str, flash_slot: &str) -> (u32, u32) {
    let roles: Vec<String> = ds
        .champions
        .iter()
        .find(|c| c.hero == hero_name || c.id == hero_name)
        .map(|c| c.roles.clone())
        .unwrap_or_default();

    let has_role = |kw: &str| roles.iter().any(|r| r.contains(kw));
    let secondary = if has_role("坦克") || has_role("战士") || has_role("刺客") {
        SPELL_MARK
    } else if has_role("射手") {
        SPELL_GHOST
    } else if has_role("辅助") {
        SPELL_EXHAUST
    } else if has_role("法师") {
        SPELL_BARRIER
    } else {
        SPELL_MARK
    };

    if flash_slot.eq_ignore_ascii_case("D") {
        (SPELL_FLASH, secondary)
    } else {
        (secondary, SPELL_FLASH)
    }
}

/// Build an ARURF (无限火力) recommendation including rune page, spells, and core items.
pub fn build_arurf_recommendation(
    ds: &Dataset,
    hero_name: &str,
    flash_slot: &str,
) -> ArurfRecommendation {
    let champ = ds
        .champions
        .iter()
        .find(|c| c.hero == hero_name || c.id == hero_name);
    let hero_id = champ.map(|c| c.id.clone()).unwrap_or_default();
    let display_name = champ
        .map(|c| c.hero.clone())
        .unwrap_or_else(|| hero_name.to_string());
    let roles = champ.map(|c| c.roles.clone()).unwrap_or_default();
    let has_role = |kw: &str| roles.iter().any(|r| r.contains(kw));

    let (role_tag, secondary_spell, rune_page, tips) = if has_role("射手") {
        (
            "射手 · 高攻速暴击".to_string(),
            SPELL_GHOST,
            RunePagePlan {
                name: format!("HexBox 无限火力 · {display_name}"),
                primary_style_id: 8000, // Precision
                sub_style_id: 8100,     // Domination
                selected_perk_ids: vec![8008, 9111, 9104, 8014, 8139, 8135, 5005, 5008, 5001],
                primary_name: "精密".to_string(),
                sub_name: "主宰".to_string(),
                keystone_name: "致命节奏".to_string(),
            },
            vec![
                "无限火力攻速收益翻倍，优先叠高暴击与吸血".to_string(),
                "带疾跑拉扯风筝，避免被刺客贴脸秒杀".to_string(),
            ],
        )
    } else if has_role("法师") {
        (
            "法师 · 爆发拉扯".to_string(),
            SPELL_BARRIER,
            RunePagePlan {
                name: format!("HexBox 无限火力 · {display_name}"),
                primary_style_id: 8100, // Domination
                sub_style_id: 8200,     // Sorcery
                selected_perk_ids: vec![8128, 8126, 8138, 8106, 8210, 8236, 5008, 5008, 5001],
                primary_name: "主宰".to_string(),
                sub_name: "巫术".to_string(),
                keystone_name: "黑暗收割".to_string(),
            },
            vec![
                "黑暗收割 + 超然/风暴聚集在高频团战中滚雪球极快".to_string(),
                "早出中娅或女妖防止刺客无限技能突脸".to_string(),
            ],
        )
    } else if has_role("坦克") {
        (
            "坦克 · 无限控制".to_string(),
            SPELL_GHOST,
            RunePagePlan {
                name: format!("HexBox 无限火力 · {display_name}"),
                primary_style_id: 8400, // Resolve
                sub_style_id: 8000,     // Precision
                selected_perk_ids: vec![8437, 8446, 8429, 8451, 9111, 9105, 5008, 5001, 5001],
                primary_name: "坚决".to_string(),
                sub_name: "精密".to_string(),
                keystone_name: "不灭之握".to_string(),
            },
            vec![
                "心之钢 + 璀璨回响/无终恨意配合高频技能极难被击杀".to_string(),
                "带爆破快速吃镀层推塔滚经济".to_string(),
            ],
        )
    } else if has_role("辅助") {
        (
            "辅助 · 护盾增益".to_string(),
            SPELL_EXHAUST,
            RunePagePlan {
                name: format!("HexBox 无限火力 · {display_name}"),
                primary_style_id: 8200, // Sorcery
                sub_style_id: 8400,     // Resolve
                selected_perk_ids: vec![8214, 8224, 8210, 8236, 8473, 8453, 5008, 5008, 5001],
                primary_name: "巫术".to_string(),
                sub_name: "坚决".to_string(),
                keystone_name: "召唤：艾黎".to_string(),
            },
            vec![
                "复苏配合无限火力高频护盾治疗收益极高".to_string(),
                "虚弱克制对面刺客进场秒人".to_string(),
            ],
        )
    } else {
        (
            "战士/刺客 · 持续进场".to_string(),
            SPELL_IGNITE,
            RunePagePlan {
                name: format!("HexBox 无限火力 · {display_name}"),
                primary_style_id: 8000, // Precision
                sub_style_id: 8400,     // Resolve
                selected_perk_ids: vec![8010, 9111, 9105, 8299, 8473, 8451, 5005, 5008, 5001],
                primary_name: "精密".to_string(),
                sub_name: "坚决".to_string(),
                keystone_name: "征服者".to_string(),
            },
            vec![
                "征服者 + 凯旋在高频对拼中持续回血续航".to_string(),
                "优先出吸血与半肉穿透装提升容错".to_string(),
            ],
        )
    };

    let (spell1_id, spell2_id) = if flash_slot.eq_ignore_ascii_case("D") {
        (SPELL_FLASH, secondary_spell)
    } else {
        (secondary_spell, SPELL_FLASH)
    };

    let core_items: Vec<FormulaItemRef> = ds
        .formula_items_by_hero
        .get(&display_name)
        .or_else(|| ds.formula_items_by_hero.get(&hero_id))
        .map(|b| b.core_items.clone())
        .unwrap_or_default();

    ArurfRecommendation {
        hero_name: display_name,
        hero_id,
        role_tag,
        spell1_id,
        spell2_id,
        spell_names: vec![
            summoner_spell_name(spell1_id).to_string(),
            summoner_spell_name(spell2_id).to_string(),
        ],
        rune_page,
        core_items,
        tips,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn respects_flash_slot_and_builds_arurf_rune_page() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .unwrap()
            .to_path_buf();
        let ds = Dataset::load_from_root(&root).expect("load dataset");
        let (s1_d, s2_d) = recommend_aram_spells(&ds, "寒冰射手", "D");
        assert_eq!(s1_d, SPELL_FLASH);
        assert_ne!(s2_d, SPELL_FLASH);

        let (s1_f, s2_f) = recommend_aram_spells(&ds, "寒冰射手", "F");
        assert_eq!(s2_f, SPELL_FLASH);
        assert_ne!(s1_f, SPELL_FLASH);

        let rec = build_arurf_recommendation(&ds, "寒冰射手", "F");
        assert_eq!(rec.rune_page.selected_perk_ids.len(), 9);
        assert_eq!(rec.spell2_id, SPELL_FLASH);
    }
}
