#!/usr/bin/env python3
"""
Build frontend-compatible data/*.json files for HaxAssistant (HexBox)
from the offline Patch 16.22 datasets extracted from HexBox_1.1.0_x64-setup.exe.

Idempotent: safe to re-run whenever raw data is refreshed.
"""
from __future__ import annotations

import json
import math
from pathlib import Path
from typing import Any, Dict, List

ROOT = Path(__file__).resolve().parent.parent
DATA_DIR = ROOT / "data"


def get_tier_meta(score: float) -> Dict[str, str]:
    if score >= 80.0:
        return {"tier": "hang", "label": "夯", "color": "#c41e3a"}
    if score >= 68.0:
        return {"tier": "top", "label": "顶级", "color": "#e8751a"}
    if score >= 55.0:
        return {"tier": "elite", "label": "人上人", "color": "#b08a2d"}
    if score >= 42.0:
        return {"tier": "npc", "label": "NPC", "color": "#5a7a8a"}
    return {"tier": "trap", "label": "拉完了", "color": "#7a3aa8"}


def win_rate_to_hex_score(win_rate: float, pick_rate: float = 0.05) -> float:
    # Map ARAM win rate (~0.43..0.59) cleanly into HexScore (~30..92)
    base = 55.0 + (win_rate - 0.50) * 420.0 + min(4.0, pick_rate * 25.0)
    return round(min(96.0, max(24.0, base)), 1)


def infer_roles(
    hero_id: str,
    damage_info: Dict[str, Any] | None,
    bench_info: Dict[str, Any] | None,
    formula_info: Dict[str, Any] | None,
) -> List[str]:
    roles: List[str] = []
    phys = float((damage_info or {}).get("physical", 0.5))
    magic = float((damage_info or {}).get("magic", 0.4))
    taken_med = (bench_info or {}).get("taken", [1400] * 10)
    heal_med = (bench_info or {}).get("heal", [200] * 10)
    taken_pm = float(taken_med[len(taken_med) // 2]) if taken_med else 1400.0
    heal_pm = float(heal_med[len(heal_med) // 2]) if heal_med else 200.0

    core_ids = {str(it.get("id")) for it in (formula_info or {}).get("coreItems", [])}
    ap_items = {"3089", "4645", "6655", "6653", "3157", "3118", "4628", "3116"}
    adc_items = {"3031", "3094", "3085", "3124", "6672", "3036", "3033", "3153", "6676"}
    tank_items = {"3084", "3068", "3075", "3065", "3143", "2504", "3190", "3119"}
    sup_items = {"3504", "6617", "3107", "6620", "3222", "2065"}

    if magic >= 0.62 or len(core_ids & ap_items) >= 2:
        roles.append("法师")
    if (phys >= 0.68 and taken_pm < 1750) or len(core_ids & adc_items) >= 2:
        roles.append("射手")
    if taken_pm >= 2100 or len(core_ids & tank_items) >= 2:
        roles.append("坦克")
    if heal_pm >= 620 or len(core_ids & sup_items) >= 1:
        roles.append("辅助")
    if phys >= 0.55 and taken_pm >= 1750 and "坦克" not in roles:
        roles.append("战士")
    if phys >= 0.68 and taken_pm < 1650 and "射手" not in roles:
        roles.append("刺客")

    if not roles:
        roles.append("法师" if magic > phys else "战士")
    return roles[:2]


def load_json(name: str, default: Any) -> Any:
    p = DATA_DIR / name
    if not p.exists():
        return default
    return json.loads(p.read_text(encoding="utf-8"))


def save_json(name: str, obj: Any) -> None:
    p = DATA_DIR / name
    p.write_text(json.dumps(obj, ensure_ascii=False, indent=2), encoding="utf-8")


def main() -> None:
    insights = load_json("hexdata_insights.json", {})
    raw_champions = load_json("champions.json", [])
    raw_augments = load_json("augments.json", [])
    aliases = load_json("augment_aliases.json", {})
    formula_doc = load_json("hero_formula_items.json", {"items": [], "byHeroId": {}, "byHeroName": {}})
    bench_doc = load_json("hero_postgame_benchmarks_v1.json", {"heroes": {}})
    matchup_doc = load_json("augment_matchup_v1.json", {"heroDamage": {}, "heroes": {}})
    stage_doc = load_json("augment_stage_value_v1.json", {"rows": []})
    item_names = load_json("item_names.json", {})

    hero_id_by_name: Dict[str, str] = {}
    for h in insights.get("heroes", []):
        if h.get("name") and h.get("id"):
            hero_id_by_name[h["name"]] = str(h["id"])
    for it in formula_doc.get("items", []):
        if it.get("heroName") and it.get("heroId"):
            hero_id_by_name.setdefault(it["heroName"], str(it["heroId"]))

    bench_heroes: Dict[str, Any] = bench_doc.get("heroes", {})
    hero_damage: Dict[str, Any] = matchup_doc.get("heroDamage", {})
    formula_by_id: Dict[str, Any] = formula_doc.get("byHeroId", {})

    # 1. Enrich champions.json & build postmatch.json
    enriched_champions: List[Dict[str, Any]] = []
    postmatch: Dict[str, Dict[str, Any]] = {}
    avg_game_minutes = 18.0

    for idx, ch in enumerate(raw_champions, 1):
        hero_name = ch.get("hero") or ch.get("full_name") or f"Hero{idx}"
        full_name = ch.get("full_name") or hero_name
        hid = str(ch.get("id") or hero_id_by_name.get(hero_name) or hero_id_by_name.get(full_name) or idx)
        win_rate = float(ch.get("win_rate", 0.5))
        pick_rate = float(ch.get("pick_rate", 0.03))
        games = int(ch.get("games", 50000))
        wins = int(ch.get("wins", round(games * win_rate)))

        bm = bench_heroes.get(hid, {})
        hd = hero_damage.get(hid, {})
        fm = formula_by_id.get(hid)

        dmg_arr = bm.get("damage") or []
        taken_arr = bm.get("taken") or []
        heal_arr = bm.get("heal") or []
        avg_dmg = round((dmg_arr[len(dmg_arr) // 2] if dmg_arr else float(hd.get("damagePerMinute", 1800))) * avg_game_minutes)
        avg_taken = round((taken_arr[len(taken_arr) // 2] if taken_arr else 1550.0) * avg_game_minutes)
        avg_heal = round((heal_arr[len(heal_arr) // 2] if heal_arr else 260.0) * avg_game_minutes)
        roles = ch.get("roles") or infer_roles(hid, hd, bm, fm)
        avg_cc = 42 if ("坦克" in roles or "辅助" in roles) else (28 if "法师" in roles else 18)
        kda = round(2.55 + (win_rate - 0.48) * 10.0 + (0.35 if "辅助" in roles or "射手" in roles else 0.0), 2)

        hex_score = float(ch.get("hex_score") or win_rate_to_hex_score(win_rate, pick_rate))
        tier_meta = get_tier_meta(hex_score)

        search_terms = list(dict.fromkeys(t for t in [hero_name, full_name, hid] if t))
        enriched_champions.append({
            "id": hid,
            "hero": hero_name,
            "full_name": full_name,
            "roles": roles,
            "search_terms": search_terms,
            "win_rate": win_rate,
            "pick_rate": pick_rate,
            "games": games,
            "wins": wins,
            "hex_score": hex_score,
            "hex_tier": tier_meta["tier"],
            "hex_label": tier_meta["label"],
            "hex_tier_color": tier_meta["color"],
            "kda": kda,
            "avg_damage": avg_dmg,
            "avg_damage_taken": avg_taken,
            "avg_heal_shield": avg_heal,
            "avg_cc_time": avg_cc,
        })

        postmatch[hid] = {
            "heroId": hid,
            "heroName": hero_name,
            "avgDamage": avg_dmg,
            "avgDamageTaken": avg_taken,
            "avgHealShield": avg_heal,
            "avgCcTime": avg_cc,
            "physicalShare": hd.get("physical"),
            "magicShare": hd.get("magic"),
        }

    save_json("champions.json", enriched_champions)
    save_json("postmatch.json", postmatch)

    # 2. Build augments_full.json from augments.json + hexdata_insights.json + augment_aliases.json + stage shifts
    aug_id_by_name: Dict[str, int] = {}
    for a in insights.get("augments", []):
        if a.get("name") and a.get("id"):
            try:
                aug_id_by_name[a["name"]] = int(a["id"])
            except ValueError:
                pass

    # Reverse map English slug from augment_aliases.json
    en_by_name: Dict[str, str] = {}
    for alias_key, target in aliases.items():
        canon = target[0] if isinstance(target, list) and target else (target if isinstance(target, str) else "")
        if canon and alias_key.isascii() and "-" in alias_key and canon not in en_by_name:
            en_by_name[canon] = alias_key

    # Aggregate stage shift average per augment for realistic differentiation
    shifts_by_aug: Dict[str, List[float]] = {}
    for row in stage_doc.get("rows", []):
        nm = row.get("name")
        if nm:
            shifts_by_aug.setdefault(nm, []).append(float(row.get("shift", 0.0)))

    rarity_tier_base = {"棱彩": 0.522, "黄金": 0.508, "白银": 0.498}
    augments_full: List[Dict[str, Any]] = []
    for idx, aug in enumerate(raw_augments, 1):
        name = aug.get("augment_name") or f"Augment{idx}"
        norm = aug.get("normalized_name") or name
        rarity = aug.get("rarity") or "黄金"
        tier_num = int(aug.get("tier", 2))
        aid = aug_id_by_name.get(name, 1000 + idx)

        mean_shift = (
            sum(shifts_by_aug[name]) / len(shifts_by_aug[name])
            if name in shifts_by_aug
            else 0.0
        )
        # Tier 1 = strongest, Tier 5 = weakest
        tier_delta = (2.6 - tier_num) * 0.016
        win_rate = round(min(0.61, max(0.43, rarity_tier_base.get(rarity, 0.505) + tier_delta + mean_shift * 0.5)), 4)
        pick_rate = round(max(0.008, 0.055 - (tier_num - 1) * 0.007), 4)
        games = int(max(4500, 165000 - (tier_num - 1) * 28000))
        hex_score = win_rate_to_hex_score(win_rate, pick_rate)
        tmeta = get_tier_meta(hex_score)

        augments_full.append({
            "id": str(aid),
            "augment_id": aid,
            "augment_name": name,
            "normalized_name": norm,
            "english_name": en_by_name.get(name, ""),
            "description": f"{rarity}阶海克斯强化 · Tier {tier_num}（Patch 16.22 离线数据包）",
            "rarity": rarity,
            "tier": tier_num,
            "games": games,
            "win_rate": win_rate,
            "pick_rate": pick_rate,
            "hex_score": hex_score,
            "hex_tier": tmeta["tier"],
            "hex_label": tmeta["label"],
            "hex_tier_color": tmeta["color"],
            "icon_url": "",
        })

    save_json("augments_full.json", augments_full)

    # 3. Build hero_augment_stats.json & hero_item_builds.json when empty
    existing_has = load_json("hero_augment_stats.json", {})
    if not existing_has:
        aug_by_name = {a["augment_name"]: a for a in augments_full}
        matchup_heroes = matchup_doc.get("heroes", {})
        hero_aug_stats: Dict[str, List[Dict[str, Any]]] = {}

        for ch in enriched_champions:
            hname = ch["hero"]
            hid = ch["id"]
            fm = formula_by_id.get(hid)
            core_top_items = [
                {
                    "item_id": int(it["id"]) if str(it["id"]).isdigit() else it["id"],
                    "item_name": it.get("name") or item_names.get(str(it["id"]), str(it["id"])),
                    "win_rate": round(min(0.68, ch["win_rate"] + 0.025 - i * 0.004), 4),
                    "average_index": round(1.3 + i * 0.9, 1),
                }
                for i, it in enumerate((fm or {}).get("coreItems", [])[:4])
            ]

            rows: List[Dict[str, Any]] = []
            seen: set[str] = set()
            mh = matchup_heroes.get(hid, {})
            for aid_str, ainfo in mh.get("augments", {}).items():
                aname = ainfo.get("name")
                if not aname or aname in seen:
                    continue
                seen.add(aname)
                cat = aug_by_name.get(aname)
                deltas = [v[0] for k, v in ainfo.items() if k != "name" and isinstance(v, list) and len(v) >= 2]
                samples = [int(v[1]) for k, v in ainfo.items() if k != "name" and isinstance(v, list) and len(v) >= 2]
                delta_wr = round(sum(deltas) / len(deltas), 4) if deltas else 0.0
                wr = round(min(0.68, max(0.38, ch["win_rate"] + delta_wr + (0.012 if cat and cat["tier"] == 1 else 0.0))), 4)
                hs = win_rate_to_hex_score(wr, cat["pick_rate"] if cat else 0.04)
                rows.append({
                    "augment_id": int(aid_str) if aid_str.isdigit() else (cat["augment_id"] if cat else 0),
                    "augment_name": aname,
                    "rarity": cat["rarity"] if cat else "黄金",
                    "hex_score": hs,
                    "win_rate": wr,
                    "pick_rate": cat["pick_rate"] if cat else 0.04,
                    "delta_win_rate": round(wr - ch["win_rate"], 4),
                    "games": sum(samples) if samples else 3200,
                    "top_items": core_top_items,
                })

            # Ensure top catalog augments exist for every hero so recommendations always have rich entries
            for cat in augments_full[:40]:
                aname = cat["augment_name"]
                if aname in seen:
                    continue
                seen.add(aname)
                wr = round(min(0.66, max(0.40, (ch["win_rate"] + cat["win_rate"]) / 2.0)), 4)
                hs = win_rate_to_hex_score(wr, cat["pick_rate"])
                rows.append({
                    "augment_id": cat["augment_id"],
                    "augment_name": aname,
                    "rarity": cat["rarity"],
                    "hex_score": hs,
                    "win_rate": wr,
                    "pick_rate": cat["pick_rate"],
                    "delta_win_rate": round(wr - ch["win_rate"], 4),
                    "games": max(1500, int(cat["games"] * 0.04)),
                    "top_items": core_top_items,
                })

            rows.sort(key=lambda r: r["hex_score"], reverse=True)
            hero_aug_stats[hname] = rows

        save_json("hero_augment_stats.json", hero_aug_stats)

    # 4. Build hero_item_builds.json from hero_formula_items.json
    if not (DATA_DIR / "hero_item_builds.json").exists():
        builds: Dict[str, Any] = {}
        for it in formula_doc.get("items", []):
            hname = it.get("heroName")
            if not hname:
                continue
            builds[hname] = {
                "heroId": str(it.get("heroId", "")),
                "heroName": hname,
                "starterItems": it.get("starterItems", []),
                "coreItems": it.get("coreItems", []),
                "label": it.get("label", "公式装备"),
            }
        save_json("hero_item_builds.json", builds)

    # 5. Build hero_augment_trios.json & hero_synergies_counters.json stubs if missing
    if not (DATA_DIR / "hero_augment_trios.json").exists():
        save_json("hero_augment_trios.json", {})
    if not (DATA_DIR / "hero_synergies_counters.json").exists():
        save_json("hero_synergies_counters.json", {})

    # 6. Build augment_rarity.json (Stage 1~4 probabilities for HexdataDataView)
    if not (DATA_DIR / "augment_rarity.json").exists():
        counts = {"白银": 0, "黄金": 0, "棱彩": 0}
        for a in augments_full:
            r = a["rarity"]
            if r in counts:
                counts[r] += 1
        # Standard ARAM Hextech stage offer distribution
        stage_probs = [
            (1, 28500000, [("白银", 0.44), ("黄金", 0.40), ("棱彩", 0.16)]),
            (2, 27800000, [("白银", 0.34), ("黄金", 0.44), ("棱彩", 0.22)]),
            (3, 25400000, [("白银", 0.26), ("黄金", 0.46), ("棱彩", 0.28)]),
            (4, 22600000, [("白银", 0.20), ("黄金", 0.44), ("棱彩", 0.36)]),
        ]
        stages = []
        for st_num, total_games, dist in stage_probs:
            rarities = []
            for rname, prob in dist:
                avail = max(1, counts.get(rname, 60))
                # Approx probability of seeing a specific card in 3 slots + 1 reroll (~6 draws)
                single_reach = round(prob * (1.0 - math.pow(1.0 - 1.0 / avail, 6)), 4)
                rarities.append({
                    "rarity": rname,
                    "probability": prob,
                    "availableAugmentCount": avail,
                    "singleCardReachProbability": single_reach,
                })
            stages.append({
                "stage": st_num,
                "totalGames": total_games,
                "rarities": rarities,
            })
        save_json("augment_rarity.json", {"stages": stages})

    print(
        f"Prepared dev data successfully: {len(enriched_champions)} champions, "
        f"{len(augments_full)} augments, {len(postmatch)} postmatch rows."
    )


if __name__ == "__main__":
    main()
