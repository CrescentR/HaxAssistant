import type {
  AppSettings,
  AugmentCatalogEntry,
  AugmentMechanicPairs,
  AugmentRecommendationCard,
  ChampionSummary,
  EnemyCounterAnalysis,
  FormulaItemBuild,
  HeroAugmentStatRow,
  HeroItemBuildPack,
  PlayerMarker,
} from "../types/hexbox";
import { getTierMeta } from "../utils/hexScore";
import { isTauriRuntime, tauriReadDataFile } from "./tauriBridge";

export interface LoadedHexBoxDatabase {
  patch: string;
  reportDate: string;
  champions: ChampionSummary[];
  augments: AugmentCatalogEntry[];
  heroAugmentStats: Record<string, HeroAugmentStatRow[]>;
  heroItemBuilds: Record<string, HeroItemBuildPack>;
  heroAugmentTrios: Record<string, any[]>;
  heroSynergiesCounters: Record<string, any>;
  formulaItemsByHero: Record<string, FormulaItemBuild>;
  skillOrdersByHeroId: Record<string, { order: string; seq: string; games: number; pick_rate: number; win_rate: number }>;
  mechanicPairs: AugmentMechanicPairs;
  stageShifts: Record<string, Record<number, number>>;
  postmatchByHeroId: Record<string, any>;
  augmentRarity: any;
  itemNames: Record<string, string>;
  itemPrices: Record<string, { total: number; base: number; from?: number[] }>;
  presets: Array<{ key: string; width: number; height: number; scale: number }>;
}

let cachedDbPromise: Promise<LoadedHexBoxDatabase> | null = null;

/** Synced copy in the app's writable data directory (desktop only). */
async function readSyncedJson<T>(fileName: string): Promise<T | undefined> {
  if (!isTauriRuntime()) return undefined;
  try {
    const text = await tauriReadDataFile(fileName);
    return text ? (JSON.parse(text) as T) : undefined;
  } catch {
    return undefined;
  }
}

/** Static copy bundled with the frontend (offline fallback, browser preview). */
async function fetchBundledJson<T>(fileName: string): Promise<T | undefined> {
  try {
    const res = await fetch(`/data/${fileName}`);
    if (!res.ok) return undefined;
    return (await res.json()) as T;
  } catch {
    return undefined;
  }
}

/**
 * `augments.json` only carries name/rarity/tier. Fill the remaining catalog
 * fields with neutral values so views built for `augments_full.json` keep working.
 */
function normalizeAugment(raw: Partial<AugmentCatalogEntry>, index: number): AugmentCatalogEntry {
  const hexScore = raw.hex_score ?? 56;
  const tierMeta = getTierMeta(hexScore);
  const name = raw.augment_name ?? "";
  return {
    ...raw,
    id: raw.id ?? raw.normalized_name ?? (name || `augment-${index}`),
    augment_id: raw.augment_id ?? 0,
    augment_name: name,
    normalized_name: raw.normalized_name ?? name,
    english_name: raw.english_name ?? "",
    description: raw.description ?? "",
    rarity: raw.rarity ?? "",
    tier: raw.tier ?? 0,
    games: raw.games ?? 0,
    win_rate: raw.win_rate ?? 0.5,
    pick_rate: raw.pick_rate ?? 0,
    hex_score: hexScore,
    hex_tier: raw.hex_tier ?? tierMeta.tier,
    hex_label: raw.hex_label ?? tierMeta.label,
    hex_tier_color: raw.hex_tier_color ?? tierMeta.color,
  };
}

/**
 * Same order as the Rust loader: `augments_full.json`, then `augments.json`,
 * trying the synced data directory before the bundled copy.
 */
async function loadAugments(hasSyncedManifest: boolean): Promise<AugmentCatalogEntry[]> {
  const readers = hasSyncedManifest ? [readSyncedJson] : [readSyncedJson, fetchBundledJson];
  for (const read of readers) {
    for (const fileName of ["augments_full.json", "augments.json"]) {
      const rows = await read<Partial<AugmentCatalogEntry>[]>(fileName);
      if (Array.isArray(rows) && rows.length > 0) return rows.map(normalizeAugment);
    }
  }
  return [];
}

/**
 * Returns the newest database. A forced reload starts a new generation; loads
 * it supersedes resolve to that newer generation, so callers can apply
 * whatever they get without an older read overwriting a newer one.
 */
export function loadDatabase(forceReload = false): Promise<LoadedHexBoxDatabase> {
  if (cachedDbPromise && !forceReload) return cachedDbPromise;

  const current: Promise<LoadedHexBoxDatabase> = readDatabase().then(
    (db) => (cachedDbPromise === current ? db : loadDatabase()),
    (err) => {
      if (cachedDbPromise !== current) return loadDatabase();
      cachedDbPromise = null; // let the next call retry
      throw err;
    }
  );
  cachedDbPromise = current;
  return current;
}

async function readDatabase(): Promise<LoadedHexBoxDatabase> {
  type Manifest = { version?: string; report_date?: string };
  const syncedManifest = await readSyncedJson<Manifest>("manifest.json");
  const hasSyncedManifest = syncedManifest != null;
  // A desktop snapshot can intentionally retire optional files. Keep those
  // absent so older bundled stats cannot reappear after a successful update.
  async function fetchJsonSafe<T>(fileName: string, fallback: T): Promise<T> {
    const data = await readSyncedJson<T>(fileName);
    if (data != null || hasSyncedManifest) return data ?? fallback;
    return (await fetchBundledJson<T>(fileName)) ?? fallback;
  }

  const [
    manifest,
    runtimeMeta,
    champions,
    augments,
    heroAugmentStats,
    heroItemBuilds,
    heroAugmentTrios,
    heroSynergiesCounters,
    formulaItemsDoc,
    skillOrdersDoc,
    mechanicPairs,
    stageValueDoc,
    postmatchByHeroId,
    augmentRarity,
    itemNames,
    itemPricesDoc,
    presets,
  ] = await Promise.all([
    syncedManifest ?? fetchBundledJson<Manifest>("manifest.json"),
    fetchJsonSafe<any>("runtime_meta.json", { version: "16.23", report_date: "2026-11-22" }),
    fetchJsonSafe<ChampionSummary[]>("champions.json", []),
    loadAugments(hasSyncedManifest),
    fetchJsonSafe<Record<string, HeroAugmentStatRow[]>>("hero_augment_stats.json", {}),
    fetchJsonSafe<Record<string, HeroItemBuildPack>>("hero_item_builds.json", {}),
    fetchJsonSafe<Record<string, any[]>>("hero_augment_trios.json", {}),
    fetchJsonSafe<Record<string, any>>("hero_synergies_counters.json", {}),
    fetchJsonSafe<any>("hero_formula_items.json", { items: [] }),
    fetchJsonSafe<any>("hero_skill_orders.json", { champions: {} }),
    fetchJsonSafe<AugmentMechanicPairs>("augment_mechanic_pairs_v1.json", {
      schemaVersion: "1",
      reviewedPatch: "16.23",
      note: "",
      links: [],
      conflicts: [],
    }),
    fetchJsonSafe<any>("augment_stage_value_v1.json", { rows: [] }),
    fetchJsonSafe<Record<string, any>>("postmatch.json", {}),
    fetchJsonSafe<any>("augment_rarity.json", { stages: [] }),
    fetchJsonSafe<Record<string, string>>("item_names.json", {}),
    fetchJsonSafe<any>("item_prices_v1.json", { items: {} }),
    fetchJsonSafe<any[]>("augment_presets.json", []),
  ]);

  if (!Array.isArray(champions) || champions.length === 0) {
    throw new Error("英雄数据为空或无法读取");
  }

  const formulaItemsByHero: Record<string, FormulaItemBuild> = {};
  for (const item of formulaItemsDoc?.items || []) {
    formulaItemsByHero[item.heroName] = item;
    formulaItemsByHero[String(item.heroId)] = item;
  }

  const stageShifts: Record<string, Record<number, number>> = {};
  for (const row of stageValueDoc?.rows || []) {
    if (!row?.name || !row?.stage) continue;
    stageShifts[row.name] = stageShifts[row.name] || {};
    stageShifts[row.name][Number(row.stage)] = Number(row.shift || 0);
  }

  return {
    patch: manifest?.version || runtimeMeta?.version || "16.23",
    reportDate: manifest?.report_date || runtimeMeta?.report_date || "2026-11-22",
    champions,
    augments,
    heroAugmentStats,
    heroItemBuilds,
    heroAugmentTrios,
    heroSynergiesCounters,
    formulaItemsByHero,
    skillOrdersByHeroId: skillOrdersDoc?.champions || {},
    mechanicPairs,
    stageShifts,
    postmatchByHeroId,
    augmentRarity,
    itemNames,
    itemPrices: itemPricesDoc?.items || {},
    presets,
  };
}

/**
 * Evaluates 3 candidate Hextech Augment cards for a given hero, choice round (Stage 1~4),
 * and already confirmed augments in the current match.
 */
export function evaluateAugmentOffer(
  db: LoadedHexBoxDatabase,
  heroName: string,
  candidateNames: string[],
  stage: number,
  confirmedAugments: string[],
  valueMode: string = "balanced"
): AugmentRecommendationCard[] {
  const heroStats = db.heroAugmentStats[heroName] || [];
  const cards: AugmentRecommendationCard[] = candidateNames.map((candName, idx) => {
    const row = heroStats.find((r) => r.augment_name === candName);
    const cat = db.augments.find((a) => a.augment_name === candName);

    const baseHex = row?.hex_score ?? cat?.hex_score ?? 56.0;
    const winRate = row?.win_rate ?? cat?.win_rate ?? 0.5;
    const pickRate = row?.pick_rate ?? cat?.pick_rate ?? 0;
    const deltaWr = row?.delta_win_rate ?? winRate - 0.5;
    const sampleSize = row?.games ?? cat?.games ?? 0;
    const rarity = row?.rarity ?? cat?.rarity ?? "黄金";
    const stageShift = db.stageShifts[candName]?.[stage] ?? 0;

    let synergyNote: string | null = null;
    let conflictNote: string | null = null;
    let synergyBonus = 0;
    let hasConflict = false;

    for (const chosen of confirmedAugments) {
      for (const link of db.mechanicPairs.links) {
        if (
          (link.aName === candName && link.bName === chosen) ||
          (link.bName === candName && link.aName === chosen)
        ) {
          synergyBonus += 8.5;
          synergyNote = `机制联动【${chosen}】(${link.family})：${link.reason}`;
        }
      }
      for (const conf of db.mechanicPairs.conflicts) {
        const chosenIsA = conf.aName === chosen;
        const candIsA = conf.aName === candName;
        const chosenInB = conf.bs.some((b) => b.name === chosen);
        const candInB = conf.bs.some((b) => b.name === candName);
        if ((chosenIsA && candInB) || (candIsA && chosenInB)) {
          hasConflict = true;
          conflictNote = `废卡冲突【${chosen}】(${conf.rule})：${conf.reason}`;
        }
      }
    }

    let modeAdjust = 0;
    let modeTag = "";
    switch (valueMode) {
      case "winrate":
        modeAdjust = deltaWr * 120;
        modeTag = " · 模式:胜率优先";
        break;
      case "highroll":
        modeAdjust = (rarity === "棱彩" ? 4.5 : rarity === "黄金" ? 1.5 : -1.0) + (synergyBonus > 0 ? 4.0 : 0);
        modeTag = " · 模式:上限联动";
        break;
      case "stable":
        modeAdjust = sampleSize >= 1500 ? 3.0 : sampleSize < 300 ? -5.0 : 0;
        modeTag = " · 模式:稳健高样本";
        break;
      default:
        break;
    }

    let finalHexScore = Math.min(100, Math.max(5, baseHex + stageShift * 180 + synergyBonus + modeAdjust));
    if (hasConflict) {
      finalHexScore = 12.0;
    }
    finalHexScore = Math.round(finalHexScore * 10) / 10;
    const tierMeta = getTierMeta(finalHexScore);

    const stageTag =
      Math.abs(stageShift) >= 0.01
        ? ` · 第 ${stage} 轮修正 ${stageShift >= 0 ? "+" : ""}${(stageShift * 100).toFixed(1)}%`
        : "";

    return {
      slot: idx + 1,
      hero_name: heroName,
      augment_id: row?.augment_id ?? cat?.augment_id ?? 0,
      augment_name: candName,
      rarity,
      hex_score: finalHexScore,
      hex_tier: tierMeta.tier,
      hex_label: tierMeta.label,
      hex_tier_color: tierMeta.color,
      win_rate: winRate,
      pick_rate: pickRate,
      delta_win_rate: deltaWr,
      sample_size: sampleSize,
      stage_shift: stageShift,
      reason: `${rarity} · 基础 Hex ${baseHex.toFixed(1)} · 净胜率 ${deltaWr >= 0 ? "+" : ""}${(deltaWr * 100).toFixed(1)}%${stageTag}${modeTag}`,
      synergy_note: synergyNote,
      conflict_note: conflictNote,
      top_items: row?.top_items?.slice(0, 4) || [],
      is_best: false,
    };
  });

  if (cards.length > 0) {
    let bestIdx = 0;
    for (let i = 1; i < cards.length; i++) {
      if (cards[i].hex_score > cards[bestIdx].hex_score) {
        bestIdx = i;
      }
    }
    cards[bestIdx].is_best = true;
  }
  return cards;
}

/**
 * Analyzes enemy 5-hero team composition to recommend dynamic counter items.
 */
export function analyzeEnemyCounter(
  db: LoadedHexBoxDatabase,
  allyHeroName: string,
  enemyHeroNames: string[]
): EnemyCounterAnalysis {
  if (enemyHeroNames.length === 0) {
    return {
      state: "waiting",
      lean: "balanced",
      physicalShare: 0.5,
      magicShare: 0.45,
      trueShare: 0.05,
      boots: null,
      antiHeal: null,
      defensiveItems: [],
      healers: [],
    };
  }

  let physW = 0;
  let magicW = 0;
  let totalDmg = 0;
  let totalHeal = 0;
  let totalTaken = 0;
  let totalCc = 0;
  const healers: string[] = [];

  for (const ename of enemyHeroNames) {
    const ch = db.champions.find((c) => c.hero === ename || c.id === ename);
    const pm = ch ? db.postmatchByHeroId[ch.id] : null;
    const dmg = pm?.avgDamage ?? ch?.avg_damage ?? 32000;
    const heal = pm?.avgHealShield ?? ch?.avg_heal_shield ?? 4500;
    const taken = pm?.avgDamageTaken ?? ch?.avg_damage_taken ?? 28000;
    const cc = pm?.avgCcTime ?? ch?.avg_cc_time ?? 25;
    const roles = ch?.roles || [];

    if (roles.includes("法师") && !roles.includes("射手")) {
      magicW += dmg * 0.8;
      physW += dmg * 0.14;
    } else if (roles.includes("射手")) {
      physW += dmg * 0.84;
      magicW += dmg * 0.1;
    } else {
      physW += dmg * 0.55;
      magicW += dmg * 0.38;
    }

    totalDmg += dmg;
    totalHeal += heal;
    totalTaken += taken;
    totalCc += cc;
    if (heal >= 9200) {
      healers.push(ch?.hero || ename);
    }
  }

  const denom = Math.max(1, totalDmg);
  const physicalShare = Math.min(0.9, Math.max(0.08, physW / denom));
  const magicShare = Math.min(0.9, Math.max(0.08, magicW / denom));
  const trueShare = Math.max(0.02, 1 - physicalShare - magicShare);

  const lean =
    physicalShare >= 0.64 ? "physical" : magicShare >= 0.64 ? "magic" : "balanced";

  const formula = db.formulaItemsByHero[allyHeroName];
  const isAp = formula?.coreItems?.some((it) =>
    ["3089", "4645", "6655", "6653", "3157", "3118"].includes(String(it.id))
  );

  const boots =
    lean === "physical"
      ? {
          itemId: 3047,
          itemName: "铁板靴",
          reason: `敌方物理伤害占比高达 ${(physicalShare * 100).toFixed(0)}%，护甲鞋减免普攻`,
          lift: 0.029,
        }
      : lean === "magic" || totalCc / enemyHeroNames.length >= 36
      ? {
          itemId: 3111,
          itemName: "水银之靴",
          reason: `敌方魔法伤害 ${(magicShare * 100).toFixed(0)}% 且场均控制 ${(totalCc / enemyHeroNames.length).toFixed(0)}s`,
          lift: 0.032,
        }
      : null;

  const antiHeal =
    healers.length >= 2 || totalHeal / enemyHeroNames.length >= 8000
      ? isAp
        ? {
            itemId: 3165,
            itemName: "莫雷洛秘典",
            reason: `敌方高回复阵容 (${healers.join(" / ") || "团队高治疗"})，重伤必出`,
            lift: 0.037,
          }
        : {
            itemId: 3033,
            itemName: "凡性的提醒",
            reason: `重伤 + 百分比穿甲克制敌方回复 (${healers.join(" / ") || "团队高治疗"})`,
            lift: 0.041,
          }
      : null;

  const defensiveItems: Array<{ itemId: number; itemName: string; reason: string; lift: number }> = [];
  if (totalTaken / enemyHeroNames.length >= 34000) {
    defensiveItems.push(
      isAp
        ? {
            itemId: 6653,
            itemName: "兰德里的折磨",
            reason: "敌方前排血量与坦度极高，持续百分比灼烧收益显著",
            lift: 0.035,
          }
        : {
            itemId: 3036,
            itemName: "多米尼克领主的致意",
            reason: "敌方前排护甲深厚，高额百分比护甲穿透必出",
            lift: 0.043,
          }
    );
  }
  if (lean === "physical") {
    defensiveItems.push(
      isAp
        ? { itemId: 3157, itemName: "中娅沙漏", reason: "规避敌方高物理刺客与集火爆发", lift: 0.03 }
        : { itemId: 6333, itemName: "死亡之舞", reason: "延迟物理爆发伤害并提供击杀回血", lift: 0.031 }
    );
  } else if (lean === "magic") {
    defensiveItems.push(
      isAp
        ? { itemId: 3102, itemName: "女妖面纱", reason: "法术护盾抵挡敌方先手开团与高额 Poke", lift: 0.028 }
        : { itemId: 3156, itemName: "玛莫提乌斯之噬", reason: "高额魔法护盾应对敌方多 AP 爆发", lift: 0.029 }
    );
  }

  return {
    state: "ready",
    lean,
    physicalShare,
    magicShare,
    trueShare,
    boots,
    antiHeal,
    defensiveItems,
    healers,
  };
}

/**
 * Computes 5v5 Lineup Winrate & Bench Swap Gain.
 */
export function evaluate5v5Lineup(
  db: LoadedHexBoxDatabase,
  allyNames: string[],
  enemyNames: string[],
  benchNames: string[],
  localHeroName: string
) {
  const scoreTeam = (team: string[]) => {
    let deltaSum = 0;
    let tanks = 0;
    let mages = 0;
    let ads = 0;
    for (const hname of team) {
      const ch = db.champions.find((c) => c.hero === hname);
      if (!ch) continue;
      deltaSum += (ch.win_rate - 0.5) * 0.72;
      if (ch.roles.includes("坦克") || ch.roles.includes("战士")) tanks++;
      if (ch.roles.includes("法师")) mages++;
      if (ch.roles.includes("射手") || ch.roles.includes("刺客")) ads++;
    }
    let synBonus = 0;
    for (let i = 0; i < team.length; i++) {
      const sc = db.heroSynergiesCounters[team[i]];
      for (const syn of sc?.teammate_synergies || []) {
        if (team.slice(i + 1).includes(syn.teammateName)) {
          synBonus += Number(syn.synergyDelta || 0) * 0.45;
        }
      }
    }
    const notes: string[] = [];
    let penalty = 0;
    if (team.length >= 4 && tanks === 0) {
      penalty += 0.024;
      notes.push("缺少前排开团与承伤 (-2.4%)");
    } else if (tanks >= 1 && mages >= 1 && ads >= 1) {
      notes.push("前排/物理/魔法结构均衡 (+1.2%)");
      synBonus += 0.012;
    }
    if (team.length >= 4 && mages >= 4) {
      penalty += 0.022;
      notes.push("AP 伤害过于单一 (-2.2%)");
    } else if (team.length >= 4 && ads >= 4 && mages === 0) {
      penalty += 0.02;
      notes.push("纯 AD 菜刀队易被堆护甲针对 (-2.0%)");
    }
    return { prob: Math.min(0.75, Math.max(0.25, 0.5 + deltaSum + synBonus - penalty)), notes };
  };

  const allyEval = scoreTeam(allyNames);
  const enemyEval = scoreTeam(enemyNames);

  let counterSum = 0;
  for (const a of allyNames) {
    const sc = db.heroSynergiesCounters[a];
    for (const row of [...(sc?.strong_against || []), ...(sc?.weak_against || [])]) {
      if (enemyNames.includes(row.opponentName)) {
        counterSum += Number(row.counterDelta || 0) * 0.35;
      }
    }
  }

  const baseWin = Math.min(0.74, Math.max(0.26, 0.5 + (allyEval.prob - enemyEval.prob)));
  const adjustedWin = Math.min(0.78, Math.max(0.22, baseWin + counterSum));
  const correction = adjustedWin - baseWin;

  const benchSuggestions = benchNames
    .map((bname) => {
      const nextTeam = allyNames.map((n) => (n === localHeroName ? bname : n));
      const nextEval = scoreTeam(nextTeam);
      const gain = nextEval.prob - allyEval.prob;
      const ch = db.champions.find((c) => c.hero === bname);
      return {
        heroName: bname,
        heroId: ch?.id || "22",
        heroWinRate: ch?.win_rate || 0.5,
        hexScore: ch?.hex_score || 60,
        hexLabel: ch?.hex_label || "人上人",
        hexColor: ch?.hex_tier_color || "#b08a2d",
        gain,
      };
    })
    .sort((a, b) => b.gain - a.gain);

  return {
    baseWin,
    adjustedWin,
    correction,
    notes: allyEval.notes,
    benchSuggestions,
  };
}

const SETTINGS_KEY = "hexbox:replica:settings:v1";
/** Keep in sync with `SETTINGS_SCHEMA_VERSION` in crates/shared-types. */
export const SETTINGS_SCHEMA_VERSION = 1;
const MARKERS_KEY = "hexbox:replica:markers:v1";

export const DEFAULT_SETTINGS: AppSettings = {
  appearance_mode: "dark",
  augment_hotkey: "F8",
  item_overlay_hotkey: "F7",
  auto_show_overlay: true,
  auto_show_formula_item_overlay: true,
  player_scores_overlay_enabled: true,
  death_shop_hint_enabled: true,
  bench_quick_grab_enabled: true,
  bench_autograb_enabled: false,
  bench_preferred_champion_ids: [157, 4, 114, 51, 22],
  auto_ocr_enabled: true,
  ocr_performance_mode: "balanced",
  selected_preset_key: "1920x1080@100",
  show_augment_stats: true,
  sgp_enabled: true,
  ready_check_auto_accept: true,
  ready_check_accept_delay_ms: 0,
  summoner_flash_slot: "F",
  auto_apply_summoner_spells: false,
  augment_value_mode: "balanced",
  data_sync_url: "",
  allow_non_lol_foreground: false,
  overlay_opacity: 95,
  overlay_zoom: 100,
  close_button_action: "ask",
  auto_check_update: true,
  auto_fix_borderless: false,
  settings_schema_version: SETTINGS_SCHEMA_VERSION,
};

/**
 * One-time upgrade of settings saved before versioning. Bench auto-grab used
 * to be saved as `true` by default, so a stored `true` is not a real choice:
 * turn it off (the user must opt in again) and keep everything else. Mirrors
 * `AppSettings::migrate_legacy` in Rust.
 */
export function migrateLegacySettings(stored: Partial<AppSettings>): AppSettings {
  const merged: AppSettings = { ...DEFAULT_SETTINGS, ...stored };
  if ((stored.settings_schema_version ?? 0) < SETTINGS_SCHEMA_VERSION) {
    merged.bench_autograb_enabled = false;
    merged.settings_schema_version = SETTINGS_SCHEMA_VERSION;
  }
  return merged;
}

export function loadSettings(): AppSettings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (!raw) return DEFAULT_SETTINGS;
    const stored = JSON.parse(raw) as Partial<AppSettings>;
    const migrated = migrateLegacySettings(stored);
    if (stored.settings_schema_version !== migrated.settings_schema_version) {
      // Persist so a later explicit opt-in is not reset again.
      try {
        localStorage.setItem(SETTINGS_KEY, JSON.stringify(migrated));
      } catch {}
    }
    return migrated;
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export function saveSettings(patch: Partial<AppSettings>): AppSettings {
  const next = { ...loadSettings(), ...patch };
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(next));
  } catch {}
  return next;
}

const DEFAULT_MARKERS: PlayerMarker[] = [
  {
    puuid: "cn-hn1-carry-001",
    summonerName: "晚风皆是意#HN1",
    tag: "carry",
    tagLabel: "大腿 · Carry",
    note: "艾欧尼亚高胜率亚索/永恩，团战切入果断，偶遇 3 局全胜",
    encounters: 3,
    updatedAt: "2026-11-22 14:20",
  },
  {
    puuid: "cn-hn1-friend-002",
    summonerName: "海克斯科技局长#6688",
    tag: "friend",
    tagLabel: "熟人 · 固定车队",
    note: "擅长前排泰坦/奥恩，主动让经济给核心",
    encounters: 12,
    updatedAt: "2026-11-21 22:15",
  },
  {
    puuid: "cn-hn1-toxic-003",
    summonerName: "压力怪别排我#9912",
    tag: "toxic",
    tagLabel: "避雷 · 压力怪",
    note: "选人阶段不给脆皮就秒退或挂机，遇到注意保重分",
    encounters: 2,
    updatedAt: "2026-11-20 19:40",
  },
];

export function loadPlayerMarkers(): PlayerMarker[] {
  try {
    const raw = localStorage.getItem(MARKERS_KEY);
    if (!raw) return DEFAULT_MARKERS;
    return JSON.parse(raw);
  } catch {
    return DEFAULT_MARKERS;
  }
}

export function savePlayerMarkers(markers: PlayerMarker[]): void {
  try {
    localStorage.setItem(MARKERS_KEY, JSON.stringify(markers));
  } catch {}
}
