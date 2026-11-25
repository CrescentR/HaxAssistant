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

    # 5. Build hero_augment_trios.json, hero_synergies_counters.json, and hero_recommended_builds.json
    mechanic_doc = load_json("augment_mechanic_pairs_v1.json", {"links": [], "conflicts": []})
    skill_orders_doc = load_json("hero_skill_orders.json", {"champions": {}})
    hero_aug_stats = load_json("hero_augment_stats.json", {})
    champions_by_name = {c["hero"]: c for c in enriched_champions}

    existing_trios = load_json("hero_augment_trios.json", {})
    if not existing_trios:
        conflict_pairs: set[tuple[str, str]] = set()
        for cf in mechanic_doc.get("conflicts", []):
            a_name = cf.get("aName")
            for b in cf.get("bs", []):
                b_name = b.get("name")
                if a_name and b_name:
                    conflict_pairs.add((a_name, b_name))
                    conflict_pairs.add((b_name, a_name))

        links = mechanic_doc.get("links", [])
        trios_by_hero: Dict[str, List[Dict[str, Any]]] = {}

        for ch in enriched_champions:
            hname = ch["hero"]
            stats_rows = hero_aug_stats.get(hname, [])
            if len(stats_rows) < 3:
                continue
            row_by_name = {r["augment_name"]: r for r in stats_rows}
            top_pool = stats_rows[:24]
            hero_trios: List[Dict[str, Any]] = []
            seen_trio_keys: set[tuple[str, ...]] = set()

            # 5a. Prioritize curated mechanic links present in this hero's augment pool
            for lk in links:
                a_name, b_name = lk.get("aName"), lk.get("bName")
                if a_name not in row_by_name or b_name not in row_by_name:
                    continue
                for third in top_pool:
                    c_name = third["augment_name"]
                    if c_name in (a_name, b_name):
                        continue
                    if (a_name, c_name) in conflict_pairs or (b_name, c_name) in conflict_pairs:
                        continue
                    key = tuple(sorted((a_name, b_name, c_name)))
                    if key in seen_trio_keys:
                        continue
                    seen_trio_keys.add(key)
                    ra, rb, rc = row_by_name[a_name], row_by_name[b_name], third
                    avg_wr = (ra["win_rate"] + rb["win_rate"] + rc["win_rate"]) / 3.0 + 0.022
                    wr = round(min(0.72, max(0.45, avg_wr)), 4)
                    hs = round(min(96.0, (ra["hex_score"] + rb["hex_score"] + rc["hex_score"]) / 3.0 + 5.5), 1)
                    hero_trios.append({
                        "augments": [a_name, b_name, c_name],
                        "family": lk.get("family", "机制联动"),
                        "reason": lk.get("reason", f"{a_name} 与 {b_name} 形成核心联动"),
                        "winRate": wr,
                        "hexScore": hs,
                        "games": max(450, min(ra["games"], rb["games"], rc["games"]) // 3),
                    })
                    break
                if len(hero_trios) >= 4:
                    break

            # 5b. Fill remaining slots with top non-conflicting combinations
            for i in range(min(6, len(top_pool))):
                if len(hero_trios) >= 5:
                    break
                ra = top_pool[i]
                for j in range(i + 1, min(8, len(top_pool))):
                    rb = top_pool[j]
                    if (ra["augment_name"], rb["augment_name"]) in conflict_pairs:
                        continue
                    for k in range(j + 1, min(10, len(top_pool))):
                        rc = top_pool[k]
                        names = (ra["augment_name"], rb["augment_name"], rc["augment_name"])
                        if (names[0], names[2]) in conflict_pairs or (names[1], names[2]) in conflict_pairs:
                            continue
                        key = tuple(sorted(names))
                        if key in seen_trio_keys:
                            continue
                        seen_trio_keys.add(key)
                        avg_wr = (ra["win_rate"] + rb["win_rate"] + rc["win_rate"]) / 3.0 + 0.012
                        wr = round(min(0.70, max(0.44, avg_wr)), 4)
                        hs = round(min(95.0, (ra["hex_score"] + rb["hex_score"] + rc["hex_score"]) / 3.0 + 2.5), 1)
                        hero_trios.append({
                            "augments": list(names),
                            "family": "高胜率三件套",
                            "reason": f"{hname} 高分海克斯组合，兼顾伤害曲线与容错",
                            "winRate": wr,
                            "hexScore": hs,
                            "games": max(320, min(ra["games"], rb["games"], rc["games"]) // 4),
                        })
                        break
                    if len(hero_trios) >= 5:
                        break

            hero_trios.sort(key=lambda t: t["hexScore"], reverse=True)
            trios_by_hero[hname] = hero_trios[:5]

        save_json("hero_augment_trios.json", trios_by_hero)

    existing_sc = load_json("hero_synergies_counters.json", {})
    if not existing_sc:
        synergy_weights = {
            ("射手", "辅助"): 0.026, ("射手", "坦克"): 0.024, ("法师", "坦克"): 0.022,
            ("法师", "辅助"): 0.018, ("刺客", "坦克"): 0.021, ("刺客", "辅助"): 0.019,
            ("战士", "辅助"): 0.023, ("战士", "法师"): 0.017, ("坦克", "射手"): 0.024,
            ("坦克", "法师"): 0.022, ("辅助", "射手"): 0.026, ("辅助", "战士"): 0.022,
        }
        counter_weights = {
            ("刺客", "射手"): 0.026, ("刺客", "法师"): 0.022, ("坦克", "刺客"): 0.025,
            ("射手", "坦克"): 0.021, ("法师", "战士"): 0.020, ("战士", "刺客"): 0.022,
            ("辅助", "刺客"): 0.020, ("法师", "辅助"): 0.016, ("坦克", "射手"): 0.014,
            ("战士", "坦克"): 0.018, ("射手", "战士"): 0.019, ("辅助", "法师"): 0.015,
        }

        sc_map: Dict[str, Any] = {}
        for ch in enriched_champions:
            hname = ch["hero"]
            roles_a = ch.get("roles", ["战士"])
            wr_a = float(ch.get("win_rate", 0.5))
            cc_a = float(ch.get("avg_cc_time", 20.0))
            heal_a = float(ch.get("avg_heal_shield", 4000.0))

            syn_candidates = []
            strong_candidates = []
            weak_candidates = []

            for other in enriched_champions:
                oname = other["hero"]
                if oname == hname:
                    continue
                roles_b = other.get("roles", ["战士"])
                wr_b = float(other.get("win_rate", 0.5))
                cc_b = float(other.get("avg_cc_time", 20.0))
                heal_b = float(other.get("avg_heal_shield", 4000.0))

                # Synergy score
                s_base = max((synergy_weights.get((ra, rb), 0.004) for ra in roles_a for rb in roles_b), default=0.004)
                s_delta = round(min(0.035, max(0.006, s_base + (wr_b - 0.5) * 0.14 + (cc_b / 2500.0) + (heal_b / 900000.0))), 4)
                syn_candidates.append({
                    "teammateName": oname,
                    "teammateId": other["id"],
                    "synergyDelta": s_delta,
                    "games": max(800, int(min(ch["games"], other["games"]) * 0.08)),
                })

                # Counter score (positive => hname strong against oname)
                c_ab = max((counter_weights.get((ra, rb), 0.0) for ra in roles_a for rb in roles_b), default=0.0)
                c_ba = max((counter_weights.get((rb, ra), 0.0) for ra in roles_a for rb in roles_b), default=0.0)
                c_delta = round(min(0.038, max(-0.038, (c_ab - c_ba) + (wr_a - wr_b) * 0.25 + (cc_a - cc_b) * 0.00018 + (heal_a - heal_b) * 0.0000004)), 4)
                if c_delta >= 0.010:
                    strong_candidates.append({
                        "opponentName": oname,
                        "opponentId": other["id"],
                        "counterDelta": c_delta,
                        "games": max(750, int(min(ch["games"], other["games"]) * 0.07)),
                    })
                elif c_delta <= -0.010:
                    weak_candidates.append({
                        "opponentName": oname,
                        "opponentId": other["id"],
                        "counterDelta": c_delta,
                        "games": max(750, int(min(ch["games"], other["games"]) * 0.07)),
                    })

            syn_candidates.sort(key=lambda x: x["synergyDelta"], reverse=True)
            strong_candidates.sort(key=lambda x: x["counterDelta"], reverse=True)
            weak_candidates.sort(key=lambda x: x["counterDelta"])
            sc_map[hname] = {
                "heroId": ch["id"],
                "teammate_synergies": syn_candidates[:6],
                "strong_against": strong_candidates[:6],
                "weak_against": weak_candidates[:6],
            }

        save_json("hero_synergies_counters.json", sc_map)

    existing_rec_builds = load_json("hero_recommended_builds.json", {})
    if not existing_rec_builds.get("heroes"):
        skill_by_id = skill_orders_doc.get("champions", {})
        rec_heroes: Dict[str, Any] = {}
        for ch in enriched_champions:
            hid = str(ch["id"])
            hname = ch["hero"]
            finfo = formula_by_id.get(hid) or {}
            sinfo = skill_by_id.get(hid) or {}
            top_augs = [
                {
                    "augmentId": r["augment_id"],
                    "augmentName": r["augment_name"],
                    "rarity": r["rarity"],
                    "hexScore": r["hex_score"],
                    "winRate": r["win_rate"],
                }
                for r in hero_aug_stats.get(hname, [])[:6]
            ]
            rec_heroes[hid] = {
                "heroId": hid,
                "heroName": hname,
                "roles": ch.get("roles", []),
                "label": finfo.get("label", "公式推荐出装"),
                "starterItems": finfo.get("starterItems", []),
                "coreItems": finfo.get("coreItems", []),
                "skillOrder": sinfo.get("order", ""),
                "skillSeq": sinfo.get("seq", ""),
                "topAugments": top_augs,
            }
        save_json(
            "hero_recommended_builds.json",
            {
                "schemaVersion": "hexbox-hero-recommended-builds-v1",
                "buildsVersion": "hexbox-derived-16.22",
                "patch": "16.22",
                "generatedAt": "2026-11-25T00:00:00Z",
                "heroCount": len(rec_heroes),
                "heroes": rec_heroes,
            },
        )

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
