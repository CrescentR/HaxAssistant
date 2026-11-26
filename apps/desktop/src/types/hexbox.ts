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
  ready_check_accept_delay_ms: number;
  summoner_flash_slot: "D" | "F" | string;
  auto_apply_summoner_spells: boolean;
  augment_value_mode: "balanced" | "winrate" | "highroll" | "stable" | string;
  data_sync_url: string;
  allow_non_lol_foreground: boolean;
  overlay_opacity: number;
  overlay_zoom: number;
  /** What the main window's × does. */
  close_button_action: "ask" | "tray" | "exit" | string;
  auto_check_update: boolean;
  /** Switch League to borderless when champ select starts. */
  auto_fix_borderless: boolean;
}

export interface AppUpdateInfo {
  available: boolean;
  currentVersion: string;
  version?: string | null;
  notes?: string | null;
  date?: string | null;
}

export interface AppUpdateProgress {
  downloaded: number;
  total?: number | null;
  finished: boolean;
}

export interface LeagueWindowMode {
  connected: boolean;
  mode?: number | null;
  label: string;
  isBorderless: boolean;
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
  locale?: string | null;
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
  ownedItemIds?: number[];
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
  damageTaken?: number;
  heal?: number;
  /** Final inventory item ids. */
  items?: number[];
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

export interface ItemPlanStep {
  itemId: string;
  itemName: string;
  kind: "starter" | "core" | "counter" | string;
  totalCost: number;
  /** Gold still needed after crediting owned components. */
  remainingCost: number;
  owned: boolean;
  reason: string;
}

export interface ItemPurchase {
  itemId: string;
  itemName: string;
  cost: number;
}

export interface ItemPlan {
  heroName: string;
  currentGold: number;
  steps: ItemPlanStep[];
  nextItem: ItemPlanStep | null;
  buyNow: ItemPurchase[];
  goldAfter: number;
  goldShort: number;
  note: string;
}

export interface PostgameMetric {
  key: "damage" | "taken" | "heal" | string;
  label: string;
  perMinute: number;
  /** 0..1 rank among games on this hero. */
  percentile: number | null;
  median: number | null;
  verdict: string;
}

export interface PostgameAugmentVerdict {
  name: string;
  hexScore: number;
  tierLabel: string;
  tierColor: string;
  winRate: number;
  games: number;
  heroSpecific: boolean;
  note: string;
}

export interface PostgameReview {
  gameId: string;
  heroName: string;
  result: string;
  grade: "S" | "A" | "B" | "C" | "D" | string;
  score: number;
  metrics: PostgameMetric[];
  benchmarkReliable: boolean;
  augments: PostgameAugmentVerdict[];
  links: string[];
  conflicts: string[];
  matchedTrio: string | null;
  betterAugments: string[];
  coreItemsBuilt: number;
  coreItemsTotal: number;
  missingCoreItems: string[];
  summary: string[];
}

export interface AssetSyncStatus {
  patch: string;
  reportDate: string;
  dataDir: string;
  modelDir: string;
  expectedDataFiles: number;
  presentDataFiles: number;
  missingDataFiles: string[];
  expectedModelFiles: number;
  presentModelFiles: number;
  missingModelFiles: string[];
  readyForOffline: boolean;
  readyForOcr: boolean;
}

export interface AssetSyncResult {
  sourceUrl: string;
  patch: string;
  reportDate: string;
  downloadedDataFiles: string[];
  downloadedModelFiles: string[];
  skippedFiles: number;
  failedFiles: string[];
  status: AssetSyncStatus;
}

export interface OcrCacheCleanupReport {
  removedFiles: number;
  freedBytes: number;
  scannedDirs: string[];
}

export interface PlayerEncounterRecord {
  id: string;
  puuid: string;
  summonerName: string;
  championName: string;
  relation: "ally" | "enemy" | string;
  result: "WIN" | "LOSS" | "UNKNOWN" | string;
  kda: string;
  timestamp: number;
  timeLabel: string;
}

export interface PlayerEncounterSummary {
  query: string;
  totalEncounters: number;
  allyGames: number;
  enemyGames: number;
  allyWins: number;
  enemyWins: number;
  marker: PlayerMarker | null;
  recent: PlayerEncounterRecord[];
}

export interface PlayerLibraryBundle {
  schemaVersion: string;
  exportedAt: number;
  markers: PlayerMarker[];
  encounters: PlayerEncounterRecord[];
}

export interface PlayerSearchResult {
  puuid: string;
  summonerId: number | null;
  summonerName: string;
  tagLine: string;
  summonerLevel: number;
  profileIconId: number;
  source: string;
  marker: PlayerMarker | null;
  encounterCount: number;
}

export interface PlayerProfile {
  summoner: PlayerSearchResult;
  source: string;
  gamesAnalyzed: number;
  winRate: number;
  avgKda: string;
  avgDamage: number;
  avgHexScore: number;
  topChampions: string[];
  recentMatches: MatchHistoryEntry[];
  encounterSummary: PlayerEncounterSummary;
}

export interface GamePlayerIntel {
  puuid: string;
  summonerName: string;
  championName: string;
  team: string;
  recentGames: number;
  recentWinRate: number | null;
  avgKda: string | null;
  avgHexScore: number | null;
  tags: string[];
  marker: PlayerMarker | null;
  encounterCount: number;
  source: string;
}

export interface IntelCacheStatus {
  cachedPlayers: number;
  sgpEnabled: boolean;
  lastRefreshEpoch: number | null;
  ttlSecs: number;
}

export interface SummonerSpellSnapshot {
  inChampSelect: boolean;
  championId: number | null;
  heroName: string | null;
  spell1Id: number | null;
  spell2Id: number | null;
  spell1Name: string;
  spell2Name: string;
  recommendedSpell1Id: number;
  recommendedSpell2Id: number;
  recommendedSpell1Name: string;
  recommendedSpell2Name: string;
  flashSlot: string;
}

export interface ItemSetSyncResult {
  heroName: string;
  championId: number;
  title: string;
  itemCount: number;
  syncedToClient: boolean;
  message: string;
}

export interface RunePagePlan {
  name: string;
  primaryStyleId: number;
  subStyleId: number;
  selectedPerkIds: number[];
  primaryName: string;
  subName: string;
  keystoneName: string;
}

export interface ArurfRecommendation {
  heroName: string;
  heroId: string;
  roleTag: string;
  spell1Id: number;
  spell2Id: number;
  spellNames: string[];
  runePage: RunePagePlan;
  coreItems: FormulaItemRef[];
  tips: string[];
}

export interface AugmentOfferStatEntry {
  augmentName: string;
  offered: number;
  picked: number;
  pickRate: number;
  lastSeenEpoch: number;
}

export interface AugmentOfferStatsReport {
  totalOffers: number;
  totalPicks: number;
  uniqueAugments: number;
  topOffered: AugmentOfferStatEntry[];
}

export interface BenchGrabStatus {
  active: boolean;
  targetChampionId: number | null;
  targetHeroName: string | null;
  statusText: string;
}
