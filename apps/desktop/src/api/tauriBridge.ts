import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  ArurfRecommendation,
  AssetSyncResult,
  AssetSyncStatus,
  AugmentOfferStatsReport,
  BenchGrabStatus,
  GamePlayerIntel,
  IntelCacheStatus,
  ItemPlan,
  ItemSetSyncResult,
  LcuStatus,
  MatchHistoryEntry,
  MatchHistoryResponse,
  OcrCacheCleanupReport,
  OverlayStatePayload,
  PlayerEncounterRecord,
  PlayerEncounterSummary,
  PlayerLibraryBundle,
  PlayerMarker,
  PlayerProfile,
  PlayerSearchResult,
  PostgameReview,
  RunePagePlan,
  RuntimeSummary,
  SummonerSpellSnapshot,
} from "../types/hexbox";

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function tauriGetRuntimeSummary(): Promise<RuntimeSummary | null> {
  if (!isTauriRuntime()) return null;
  return invoke<RuntimeSummary>("get_runtime_summary");
}

export async function tauriReadDataFile(fileName: string): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  return invoke<string>("read_data_file", { fileName });
}

export async function tauriGetSettings(): Promise<AppSettings | null> {
  if (!isTauriRuntime()) return null;
  return invoke<AppSettings>("get_settings");
}

export async function tauriUpdateSettings(settings: AppSettings): Promise<AppSettings | null> {
  if (!isTauriRuntime()) return null;
  return invoke<AppSettings>("update_settings", { settings });
}

export async function tauriGetOverlayState(): Promise<OverlayStatePayload | null> {
  if (!isTauriRuntime()) return null;
  return invoke<OverlayStatePayload>("get_overlay_state");
}

export async function tauriSetOverlayState(
  payload: OverlayStatePayload
): Promise<OverlayStatePayload | null> {
  if (!isTauriRuntime()) return null;
  return invoke<OverlayStatePayload>("set_overlay_state", { payload });
}

export async function tauriProbeLcuStatus(): Promise<LcuStatus | null> {
  if (!isTauriRuntime()) return null;
  return invoke<LcuStatus>("probe_lcu_status");
}

export async function tauriAcceptReadyCheck(): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  await invoke<void>("accept_ready_check");
  return true;
}

export async function tauriBenchSwap(championId: number): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  await invoke<void>("bench_swap", { championId });
  return true;
}

export async function tauriGetMatchHistory(
  count = 20
): Promise<MatchHistoryResponse | null> {
  if (!isTauriRuntime()) return null;
  return invoke<MatchHistoryResponse>("get_match_history", { count });
}

/** Ordered shopping plan; omitted fields come from the live game state. */
export async function tauriGetItemPlan(
  args: {
    heroName?: string;
    currentGold?: number;
    ownedItemIds?: number[];
    enemyNames?: string[];
  } = {}
): Promise<ItemPlan | null> {
  if (!isTauriRuntime()) return null;
  return invoke<ItemPlan>("get_item_recommendations", {
    heroName: args.heroName ?? null,
    currentGold: args.currentGold ?? null,
    ownedItemIds: args.ownedItemIds ?? null,
    enemyNames: args.enemyNames ?? null,
  });
}

export async function tauriGetPostgameReview(
  entry: MatchHistoryEntry
): Promise<PostgameReview | null> {
  if (!isTauriRuntime()) return null;
  return invoke<PostgameReview>("get_postgame_augment_review", { entry });
}

export async function tauriRunOcrPreset(
  presetKey?: string,
  imagePath?: string
): Promise<Record<string, unknown> | null> {
  if (!isTauriRuntime()) return null;
  return invoke<Record<string, unknown>>("run_ocr_preset", {
    presetKey: presetKey ?? null,
    imagePath: imagePath ?? null,
  });
}

export async function tauriShowOverlay(kind: string): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  await invoke<void>("show_overlay", { kind });
  return true;
}

export async function tauriHideOverlay(kind: string): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  await invoke<void>("hide_overlay", { kind });
  return true;
}

export async function tauriToggleOverlay(kind: string): Promise<boolean | null> {
  if (!isTauriRuntime()) return null;
  return invoke<boolean>("toggle_overlay", { kind });
}

/** Step overlay zoom in (+1) or out (-1); the host resizes the windows to match. */
export async function tauriZoomOverlay(direction: 1 | -1): Promise<AppSettings | null> {
  if (!isTauriRuntime()) return null;
  return invoke<AppSettings>("zoom_overlay", { direction });
}

export async function tauriResetOverlayZoom(): Promise<AppSettings | null> {
  if (!isTauriRuntime()) return null;
  return invoke<AppSettings>("reset_overlay_zoom");
}

/** Forget saved overlay positions; without `kind`, all overlays are re-centred. */
export async function tauriResetOverlayPosition(kind?: string): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  await invoke<void>("reset_overlay_position", { kind: kind ?? null });
  return true;
}

export async function tauriStartOverlayDrag(): Promise<void> {
  if (!isTauriRuntime()) return;
  await invoke<void>("start_overlay_drag");
}

export async function subscribeLcuStatus(
  handler: (status: LcuStatus) => void
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) return () => {};
  return listen<LcuStatus>("hexbox://lcu-status", (event) => {
    handler(event.payload);
  });
}

export async function subscribeOverlayState(
  handler: (payload: OverlayStatePayload) => void
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) return () => {};
  return listen<OverlayStatePayload>("hexbox://overlay-state", (event) => {
    handler(event.payload);
  });
}

export async function subscribeSettingsUpdated(
  handler: (settings: AppSettings) => void
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) return () => {};
  return listen<AppSettings>("hexbox://settings-updated", (event) => {
    handler(event.payload);
  });
}

// ---------------------------------------------------------------------------
// P0: Asset sync, OCR cache cleanup & client locale
// ---------------------------------------------------------------------------

export async function tauriGetAssetSyncStatus(): Promise<AssetSyncStatus | null> {
  if (!isTauriRuntime()) return null;
  return invoke<AssetSyncStatus>("get_asset_sync_status");
}

export async function tauriSyncRemoteAssets(
  sourceUrl?: string,
  includeModels = true
): Promise<AssetSyncResult | null> {
  if (!isTauriRuntime()) return null;
  return invoke<AssetSyncResult>("sync_remote_assets", {
    sourceUrl: sourceUrl ?? null,
    includeModels,
  });
}

export async function tauriCleanupOcrCache(): Promise<OcrCacheCleanupReport | null> {
  if (!isTauriRuntime()) return null;
  return invoke<OcrCacheCleanupReport>("cleanup_ocr_cache");
}

export async function tauriGetClientLocale(): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  return invoke<string>("get_client_locale");
}

// ---------------------------------------------------------------------------
// P1: Player markers, encounters & library import/export
// ---------------------------------------------------------------------------

export async function tauriListPlayerMarkers(): Promise<PlayerMarker[] | null> {
  if (!isTauriRuntime()) return null;
  return invoke<PlayerMarker[]>("list_player_markers");
}

export async function tauriSavePlayerMarker(marker: PlayerMarker): Promise<PlayerMarker | null> {
  if (!isTauriRuntime()) return null;
  return invoke<PlayerMarker>("save_player_marker", { marker });
}

export async function tauriDeletePlayerMarker(puuidOrName: string): Promise<boolean | null> {
  if (!isTauriRuntime()) return null;
  return invoke<boolean>("delete_player_marker", { puuidOrName });
}

export async function tauriListPlayerEncounters(
  limit = 100
): Promise<PlayerEncounterRecord[] | null> {
  if (!isTauriRuntime()) return null;
  return invoke<PlayerEncounterRecord[]>("list_player_encounters", { limit });
}

export async function tauriGetEncounterSummary(
  query: string
): Promise<PlayerEncounterSummary | null> {
  if (!isTauriRuntime()) return null;
  return invoke<PlayerEncounterSummary>("get_encounter_summary", { query });
}

export async function tauriDeletePlayerEncounters(puuidOrName: string): Promise<number | null> {
  if (!isTauriRuntime()) return null;
  return invoke<number>("delete_player_encounters", { puuidOrName });
}

export async function tauriPrunePlayerEncounters(maxAgeDays: number): Promise<number | null> {
  if (!isTauriRuntime()) return null;
  return invoke<number>("prune_player_encounters", { maxAgeDays });
}

export async function tauriExportPlayerLibrary(): Promise<PlayerLibraryBundle | null> {
  if (!isTauriRuntime()) return null;
  return invoke<PlayerLibraryBundle>("export_player_library");
}

export async function tauriImportPlayerLibrary(
  bundle: PlayerLibraryBundle,
  replace = false
): Promise<[number, number] | null> {
  if (!isTauriRuntime()) return null;
  return invoke<[number, number]>("import_player_library", { bundle, replace });
}

// ---------------------------------------------------------------------------
// P1: SGP / LCU player query & 10-player live intel
// ---------------------------------------------------------------------------

export async function tauriSearchPlayer(query: string): Promise<PlayerSearchResult | null> {
  if (!isTauriRuntime()) return null;
  return invoke<PlayerSearchResult>("search_player", { query });
}

export async function tauriGetPlayerProfile(
  query: string,
  count = 15
): Promise<PlayerProfile | null> {
  if (!isTauriRuntime()) return null;
  return invoke<PlayerProfile>("get_player_profile", { query, count });
}

export async function tauriGetGamePlayers(): Promise<GamePlayerIntel[] | null> {
  if (!isTauriRuntime()) return null;
  return invoke<GamePlayerIntel[]>("get_game_players");
}

export async function tauriRefreshGamePlayerIntel(): Promise<GamePlayerIntel[] | null> {
  if (!isTauriRuntime()) return null;
  return invoke<GamePlayerIntel[]>("refresh_game_player_intel");
}

export async function tauriGetIntelCacheStatus(): Promise<IntelCacheStatus | null> {
  if (!isTauriRuntime()) return null;
  return invoke<IntelCacheStatus>("get_intel_cache_status");
}

// ---------------------------------------------------------------------------
// P1: Summoner spells, Item Set sync, ARURF & Rune Page
// ---------------------------------------------------------------------------

export async function tauriGetSummonerSpellSnapshot(): Promise<SummonerSpellSnapshot | null> {
  if (!isTauriRuntime()) return null;
  return invoke<SummonerSpellSnapshot>("get_summoner_spell_snapshot");
}

export async function tauriApplySummonerSpells(
  spell1Id?: number,
  spell2Id?: number
): Promise<SummonerSpellSnapshot | null> {
  if (!isTauriRuntime()) return null;
  return invoke<SummonerSpellSnapshot>("apply_summoner_spells", {
    spell1Id: spell1Id ?? null,
    spell2Id: spell2Id ?? null,
  });
}

export async function tauriSyncItemSetToClient(
  heroName?: string
): Promise<ItemSetSyncResult | null> {
  if (!isTauriRuntime()) return null;
  return invoke<ItemSetSyncResult>("sync_item_set_to_client", {
    heroName: heroName ?? null,
  });
}

export async function tauriGetArurfRecommendation(
  heroName?: string
): Promise<ArurfRecommendation | null> {
  if (!isTauriRuntime()) return null;
  return invoke<ArurfRecommendation>("get_arurf_recommendation", {
    heroName: heroName ?? null,
  });
}

export async function tauriApplyArurfRunePage(heroName?: string): Promise<RunePagePlan | null> {
  if (!isTauriRuntime()) return null;
  return invoke<RunePagePlan>("apply_arurf_rune_page", {
    heroName: heroName ?? null,
  });
}

// ---------------------------------------------------------------------------
// P1: Manual augment selection/override, offer stats & bench grab
// ---------------------------------------------------------------------------

export async function tauriSetManualAugmentSelection(
  confirmedAugments: string[]
): Promise<OverlayStatePayload | null> {
  if (!isTauriRuntime()) return null;
  return invoke<OverlayStatePayload>("set_manual_augment_selection", { confirmedAugments });
}

export async function tauriSetManualAugmentOverride(
  slot: number,
  augmentName: string
): Promise<OverlayStatePayload | null> {
  if (!isTauriRuntime()) return null;
  return invoke<OverlayStatePayload>("set_manual_augment_override", { slot, augmentName });
}

export async function tauriGetAugmentOfferStats(
  topN = 30
): Promise<AugmentOfferStatsReport | null> {
  if (!isTauriRuntime()) return null;
  return invoke<AugmentOfferStatsReport>("get_augment_offer_stats", { topN });
}

export async function tauriGrabBenchChampion(championId: number): Promise<BenchGrabStatus | null> {
  if (!isTauriRuntime()) return null;
  return invoke<BenchGrabStatus>("grab_bench_champion", { championId });
}

export async function tauriCancelBenchGrab(): Promise<BenchGrabStatus | null> {
  if (!isTauriRuntime()) return null;
  return invoke<BenchGrabStatus>("cancel_bench_grab");
}

export async function tauriGetBenchGrabStatus(): Promise<BenchGrabStatus | null> {
  if (!isTauriRuntime()) return null;
  return invoke<BenchGrabStatus>("get_bench_grab_status");
}
