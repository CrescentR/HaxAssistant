export interface AppSettings {
  appearance_mode: "dark" | "light";
  augment_hotkey: string;
  item_overlay_hotkey: string;
  auto_show_overlay: boolean;
  auto_show_formula_item_overlay: boolean;
  player_scores_overlay_enabled: boolean;
  death_shop_hint_enabled: boolean;
  bench_quick_grab_enabled: boolean;
  bench_autograb_enabled: boolean;
  bench_preferred_champion_ids: number[];
  auto_ocr_enabled: boolean;
  ocr_performance_mode: "eco" | "balanced" | "performance";
  selected_preset_key: string;
  show_augment_stats: boolean;
  sgp_enabled: boolean;
  ready_check_auto_accept: boolean;
  overlay_opacity: number;
  overlay_zoom: number;
}

export interface ChampionSummary {
  id: string;
  hero: string;
  full_name?: string;
  roles: string[];
  search_terms?: string[];
  win_rate: number;
  pick_rate: number;
  games: number;
  wins?: number;
  hex_score: number;
  hex_tier?: string;
  hex_label: string;
  hex_tier_color: string;
  kda?: number;
  avg_damage?: number;
  avg_damage_taken?: number;
  avg_heal_shield?: number;
  avg_cc_time?: number;
}

export interface AugmentCatalogEntry {
  id: string;
  augment_id: number;
  augment_name: string;
  normalized_name: string;
  english_name: string;
  description: string;
  rarity: string;
  tier: number;
  games: number;
  win_rate: number;
  pick_rate: number;
  hex_score: number;
  hex_tier: string;
  hex_label: string;
  hex_tier_color: string;
  icon_url?: string;
}

export interface AugmentTopItem {
  item_id: number | string;
  item_name: string;
  win_rate: number;
  average_index: number;
}

export interface HeroAugmentStatRow {
  augment_id: number;
  augment_name: string;
  rarity: string;
  hex_score: number;
  win_rate: number;
  pick_rate: number;
  delta_win_rate: number;
  games: number;
  top_items?: AugmentTopItem[];
}

export interface AugmentMechanicLink {
  a?: string | number;
  aName: string;
  b?: string | number;
  bName: string;
  family: string;
  reason: string;
}

export interface AugmentMechanicConflict {
  a?: string | number;
  aName: string;
  rule: string;
  reason: string;
  bs: Array<{ id: string | number; name: string }>;
}

export interface AugmentMechanicPairs {
  schemaVersion: string;
  reviewedPatch: string;
  reviewedAt?: string;
  note: string;
  links: AugmentMechanicLink[];
  conflicts: AugmentMechanicConflict[];
}

export interface AugmentRecommendationCard {
  slot: number;
  hero_name: string;
  augment_id: number;
  augment_name: string;
  rarity: string;
  hex_score: number;
  hex_tier: string;
  hex_label: string;
  hex_tier_color: string;
  win_rate: number;
  pick_rate: number;
  delta_win_rate: number;
  sample_size: number;
  stage_shift: number;
  reason: string;
  synergy_note: string | null;
  conflict_note: string | null;
  top_items: AugmentTopItem[];
  is_best: boolean;
}

export interface EnemyCounterItemSuggestion {
  itemId: number;
  itemName: string;
  reason: string;
  lift: number;
}

export interface EnemyCounterAnalysis {
  state: "waiting" | "ready";
  lean: "physical" | "magic" | "balanced";
  physicalShare: number;
  magicShare: number;
  trueShare: number;
  boots: EnemyCounterItemSuggestion | null;
  antiHeal: EnemyCounterItemSuggestion | null;
  defensiveItems: EnemyCounterItemSuggestion[];
  healers: string[];
}

export interface FormulaItemRef {
  id: string;
  name: string;
}

export interface FormulaItemBuild {
  heroId: string;
  heroName: string;
  label: string;
  starterItems: FormulaItemRef[];
  coreItems: FormulaItemRef[];
}

export interface HeroItemBuildPack {
  heroId: string;
  heroName: string;
  label?: string;
  starterItems: FormulaItemRef[];
  coreItems: FormulaItemRef[];
}

export interface PlayerMarker {
  puuid: string;
  summonerName: string;
  tag: "carry" | "friend" | "toxic" | string;
  tagLabel: string;
  note: string;
  encounters: number;
  updatedAt: string;
}

export interface LcuStatus {
  connected: boolean;
  phase: string;
  port: number | null;
  summoner_name: string | null;
  lockfile_path: string | null;
}

export interface LivePlayerScore {
  summonerName: string;
  championName: string;
  team: string;
  kills: number;
  deaths: number;
  assists: number;
  isDead: boolean;
}

export interface OverlayStatePayload {
  heroName: string;
  phase: string;
  stage: number;
  candidateAugments: string[];
  confirmedAugments: string[];
  allyTeam: string[];
  enemyTeam: string[];
  benchPool: string[];
  currentGold: number;
  isDead: boolean;
  lcu: LcuStatus;
  livePlayers: LivePlayerScore[];
  ocrStatus: string | null;
  ocrMs: number | null;
}

export interface MatchHistoryEntry {
  gameId: string;
  championId: number;
  heroName: string;
  result: "WIN" | "LOSS" | string;
  kills: number;
  deaths: number;
  assists: number;
  kda: string;
  damage: number;
  durationSecs: number;
  duration: string;
  queueId: number;
  gameMode: string;
  augments: string[];
  hexScore: number;
  gameCreation: number;
  timeAgo: string;
}

export interface MatchHistoryResponse {
  source: "lcu" | "offline" | string;
  summonerName: string | null;
  matches: MatchHistoryEntry[];
}

export interface RuntimeSummary {
  patch: string;
  report_date: string;
  hero_count: number;
  augment_count: number;
  mechanic_link_count: number;
  mechanic_conflict_count: number;
  data_root: string;
}
