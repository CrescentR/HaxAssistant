#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri v2 Desktop Host Entrypoint (`apps/desktop/src-tauri/src/main.rs`)
//! Bridges `app-core` (`HexBoxCore`), `reco-engine`, `lcu-client`, and persistent `ocr-sidecar`
//! to the React frontend and overlay windows.

mod overlay_layout;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use app_core::HexBoxCore;
use serde_json::Value;
use shared_types::{
    AppSettings, ArurfRecommendation, AssetSyncResult, AssetSyncStatus, AugmentOfferStatsReport,
    AugmentRecommendationCard, BenchGrabStatus, EnemyCounterAnalysis, GamePlayerIntel,
    IntelCacheStatus, ItemPlan, ItemSetSyncResult, LcuStatus, LineupEvaluation, MatchHistoryEntry,
    MatchHistoryResponse, OcrCacheCleanupReport, OverlayStatePayload, PlayerEncounterRecord,
    PlayerEncounterSummary, PlayerLibraryBundle, PlayerMarker, PlayerProfile, PlayerSearchResult,
    PostgameReview, RunePagePlan, RuntimeSummary, SummonerSpellSnapshot,
};
use overlay_layout::{OverlayLayoutStore, SavedPosition};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow, WindowEvent};
use tokio::sync::Notify;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

struct AppState {
    core: Arc<HexBoxCore>,
    overlay_layout: Arc<OverlayLayoutStore>,
}

fn resolve_app_root(app: &AppHandle) -> PathBuf {
    let has_data = |dir: &PathBuf| {
        dir.join("data/champions.json").exists() || dir.join("data/runtime_meta.json").exists()
    };
    let manifest_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    // The source checkout wins in debug builds so dev runs use the live data
    // and the sidecar's virtualenv instead of the copies staged for bundling.
    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(debug_assertions) {
        candidates.push(manifest_root.clone());
    }
    candidates.extend(exe_dir);
    candidates.extend(app.path().resource_dir().ok());
    candidates.push(manifest_root.clone());
    candidates.extend(std::env::current_dir().ok());

    candidates
        .into_iter()
        .find(has_data)
        .unwrap_or(manifest_root)
}

#[tauri::command]
fn get_runtime_summary(state: State<'_, AppState>) -> RuntimeSummary {
    state.core.runtime_summary()
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.core.get_settings()
}

#[tauri::command]
fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let saved = state
        .core
        .update_settings(settings)
        .map_err(|e| e.to_string())?;
    register_hotkeys(&app, state.core.clone(), &saved);
    overlay_layout::apply_zoom(&app, saved.overlay_zoom);
    let _ = app.emit("hexbox://settings-updated", &saved);
    Ok(saved)
}

fn save_overlay_zoom(
    app: &AppHandle,
    state: &State<'_, AppState>,
    zoom: u16,
) -> Result<AppSettings, String> {
    let mut settings = state.core.get_settings();
    settings.overlay_zoom = zoom;
    let saved = state
        .core
        .update_settings(settings)
        .map_err(|e| e.to_string())?;
    overlay_layout::apply_zoom(app, saved.overlay_zoom);
    let _ = app.emit("hexbox://settings-updated", &saved);
    Ok(saved)
}

/// Step the overlay zoom in or out; `direction` is +1 or -1.
#[tauri::command]
fn zoom_overlay(
    app: AppHandle,
    state: State<'_, AppState>,
    direction: i32,
) -> Result<AppSettings, String> {
    let current = state.core.get_settings().overlay_zoom as i32;
    let next = overlay_layout::clamp_zoom(current + direction.signum() * overlay_layout::ZOOM_STEP);
    save_overlay_zoom(&app, &state, next)
}

#[tauri::command]
fn reset_overlay_zoom(app: AppHandle, state: State<'_, AppState>) -> Result<AppSettings, String> {
    save_overlay_zoom(&app, &state, overlay_layout::ZOOM_DEFAULT)
}

/// Forget where overlays were dragged to and centre them again. Without
/// `kind`, every overlay is reset.
#[tauri::command]
fn reset_overlay_position(app: AppHandle, state: State<'_, AppState>, kind: Option<String>) {
    let label = kind.as_deref().map(overlay_label);
    overlay_layout::reset_positions(&app, &state.overlay_layout, label);
}

/// Called from an overlay's header on mouse-down so the frameless window can be moved.
#[tauri::command]
fn start_overlay_drag(window: WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_overlay_state(state: State<'_, AppState>) -> OverlayStatePayload {
    state.core.get_overlay_state()
}

#[tauri::command]
fn set_overlay_state(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: OverlayStatePayload,
) -> OverlayStatePayload {
    let saved = state.core.set_overlay_state(payload);
    let _ = app.emit("hexbox://overlay-state", &saved);
    saved
}

#[tauri::command]
fn evaluate_augment_offer(
    state: State<'_, AppState>,
    hero_name: String,
    candidate_names: Vec<String>,
    stage: u8,
    confirmed_augments: Vec<String>,
) -> Vec<AugmentRecommendationCard> {
    state
        .core
        .evaluate_augment_offer(&hero_name, &candidate_names, stage, &confirmed_augments)
}

#[tauri::command]
fn analyze_enemy_counter(
    state: State<'_, AppState>,
    ally_hero_name: String,
    enemy_hero_names: Vec<String>,
) -> EnemyCounterAnalysis {
    state
        .core
        .analyze_enemy_counter(&ally_hero_name, &enemy_hero_names)
}

#[tauri::command]
fn evaluate_5v5_lineup(
    state: State<'_, AppState>,
    ally_names: Vec<String>,
    enemy_names: Vec<String>,
    bench_names: Vec<String>,
    local_hero_name: String,
) -> LineupEvaluation {
    state
        .core
        .evaluate_5v5_lineup(&ally_names, &enemy_names, &bench_names, &local_hero_name)
}

/// Ordered shopping plan. Every argument is optional; omitted ones come from
/// the live game state, so overlays can call it with no arguments.
#[tauri::command]
fn get_item_recommendations(
    state: State<'_, AppState>,
    hero_name: Option<String>,
    current_gold: Option<f64>,
    owned_item_ids: Option<Vec<u32>>,
    enemy_names: Option<Vec<String>>,
) -> ItemPlan {
    state.core.item_plan(
        hero_name.as_deref(),
        current_gold,
        owned_item_ids.as_deref(),
        enemy_names.as_deref(),
    )
}

#[tauri::command]
fn get_postgame_augment_review(
    state: State<'_, AppState>,
    entry: MatchHistoryEntry,
) -> PostgameReview {
    state.core.postgame_review(&entry)
}

#[tauri::command]
async fn probe_lcu_status(state: State<'_, AppState>) -> Result<LcuStatus, String> {
    Ok(state.core.probe_lcu().await)
}

#[tauri::command]
async fn accept_ready_check(state: State<'_, AppState>) -> Result<(), String> {
    state
        .core
        .accept_ready_check()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn bench_swap(state: State<'_, AppState>, champion_id: u32) -> Result<(), String> {
    state
        .core
        .bench_swap(champion_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_match_history(
    state: State<'_, AppState>,
    count: Option<usize>,
) -> Result<MatchHistoryResponse, String> {
    Ok(state
        .core
        .fetch_personal_match_history(count.unwrap_or(20))
        .await)
}

#[tauri::command]
async fn run_ocr_preset(
    app: AppHandle,
    state: State<'_, AppState>,
    preset_key: Option<String>,
    image_path: Option<String>,
) -> Result<Value, String> {
    let core = state.core.clone();
    let preset = preset_key.unwrap_or_else(|| core.get_settings().selected_preset_key);
    let res = tokio::task::spawn_blocking(move || core.run_ocr_preset(&preset, image_path.as_deref()))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let overlay_state = state.core.get_overlay_state();
    let _ = app.emit("hexbox://overlay-state", &overlay_state);
    Ok(res)
}

#[tauri::command]
async fn ocr_watch_tick(
    app: AppHandle,
    state: State<'_, AppState>,
    preset_key: Option<String>,
    image_path: Option<String>,
) -> Result<Value, String> {
    let core = state.core.clone();
    let preset = preset_key.unwrap_or_else(|| core.get_settings().selected_preset_key);
    let res =
        tokio::task::spawn_blocking(move || core.ocr_watch_tick(&preset, image_path.as_deref()))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
    let overlay_state = state.core.get_overlay_state();
    let _ = app.emit("hexbox://overlay-state", &overlay_state);
    Ok(res)
}

#[tauri::command]
async fn reset_ocr_watcher(state: State<'_, AppState>) -> Result<Value, String> {
    let core = state.core.clone();
    tokio::task::spawn_blocking(move || core.reset_ocr_watcher())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

fn overlay_label(kind: &str) -> &'static str {
    match kind {
        "augment" => "augment-overlay",
        "item" => "formula-item-overlay",
        "champ-select" => "champ-select-overlay",
        "player-scores" => "player-scores-overlay",
        "death-hint" => "death-hint-overlay",
        _ => "augment-overlay",
    }
}

fn set_overlay_visible(app: &AppHandle, kind: &str, visible: bool) {
    let label = overlay_label(kind);
    if let Some(win) = app.get_webview_window(label) {
        if visible {
            let _ = win.show();
        } else {
            let _ = win.hide();
        }
    }
}

#[tauri::command]
fn get_asset_sync_status(state: State<'_, AppState>) -> AssetSyncStatus {
    state.core.get_asset_sync_status()
}

#[tauri::command]
async fn sync_remote_assets(
    state: State<'_, AppState>,
    source_url: Option<String>,
    include_models: Option<bool>,
) -> Result<AssetSyncResult, String> {
    state
        .core
        .sync_remote_assets(source_url.as_deref(), include_models.unwrap_or(true))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn cleanup_ocr_cache(state: State<'_, AppState>) -> OcrCacheCleanupReport {
    state.core.cleanup_ocr_cache()
}

#[tauri::command]
async fn get_client_locale(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.core.get_client_locale().await)
}

#[tauri::command]
fn list_player_markers(state: State<'_, AppState>) -> Vec<PlayerMarker> {
    state.core.list_player_markers()
}

#[tauri::command]
fn save_player_marker(
    state: State<'_, AppState>,
    marker: PlayerMarker,
) -> Result<PlayerMarker, String> {
    state
        .core
        .save_player_marker(marker)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_player_marker(
    state: State<'_, AppState>,
    puuid_or_name: String,
) -> Result<bool, String> {
    state
        .core
        .delete_player_marker(&puuid_or_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_player_encounters(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Vec<PlayerEncounterRecord> {
    state.core.list_player_encounters(limit.unwrap_or(100))
}

#[tauri::command]
fn get_encounter_summary(state: State<'_, AppState>, query: String) -> PlayerEncounterSummary {
    state.core.get_encounter_summary(&query)
}

#[tauri::command]
fn delete_player_encounters(
    state: State<'_, AppState>,
    puuid_or_name: String,
) -> Result<usize, String> {
    state
        .core
        .delete_player_encounters(&puuid_or_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn prune_player_encounters(
    state: State<'_, AppState>,
    max_age_days: u64,
) -> Result<usize, String> {
    state
        .core
        .prune_player_encounters(max_age_days)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn export_player_library(state: State<'_, AppState>) -> PlayerLibraryBundle {
    state.core.export_player_library()
}

#[tauri::command]
fn import_player_library(
    state: State<'_, AppState>,
    bundle: PlayerLibraryBundle,
    replace: Option<bool>,
) -> Result<(usize, usize), String> {
    state
        .core
        .import_player_library(bundle, replace.unwrap_or(false))
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn search_player(
    state: State<'_, AppState>,
    query: String,
) -> Result<PlayerSearchResult, String> {
    state
        .core
        .search_player(&query)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_player_profile(
    state: State<'_, AppState>,
    query: String,
    count: Option<usize>,
) -> Result<PlayerProfile, String> {
    state
        .core
        .get_player_profile(&query, count.unwrap_or(15))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_game_players(state: State<'_, AppState>) -> Result<Vec<GamePlayerIntel>, String> {
    Ok(state.core.get_game_players(false).await)
}

#[tauri::command]
async fn refresh_game_player_intel(
    state: State<'_, AppState>,
) -> Result<Vec<GamePlayerIntel>, String> {
    Ok(state.core.get_game_players(true).await)
}

#[tauri::command]
fn get_intel_cache_status(state: State<'_, AppState>) -> IntelCacheStatus {
    state.core.get_intel_cache_status()
}

#[tauri::command]
async fn get_summoner_spell_snapshot(
    state: State<'_, AppState>,
) -> Result<SummonerSpellSnapshot, String> {
    Ok(state.core.get_summoner_spell_snapshot().await)
}

#[tauri::command]
async fn apply_summoner_spells(
    state: State<'_, AppState>,
    spell1_id: Option<u32>,
    spell2_id: Option<u32>,
) -> Result<SummonerSpellSnapshot, String> {
    state
        .core
        .apply_summoner_spells(spell1_id, spell2_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn sync_item_set_to_client(
    state: State<'_, AppState>,
    hero_name: Option<String>,
) -> Result<ItemSetSyncResult, String> {
    state
        .core
        .sync_item_set_to_client(hero_name.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_arurf_recommendation(
    state: State<'_, AppState>,
    hero_name: Option<String>,
) -> ArurfRecommendation {
    state.core.get_arurf_recommendation(hero_name.as_deref())
}

#[tauri::command]
async fn apply_arurf_rune_page(
    state: State<'_, AppState>,
    hero_name: Option<String>,
) -> Result<RunePagePlan, String> {
    state
        .core
        .apply_arurf_rune_page(hero_name.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_manual_augment_selection(
    app: AppHandle,
    state: State<'_, AppState>,
    confirmed_augments: Vec<String>,
) -> OverlayStatePayload {
    let saved = state.core.set_manual_augment_selection(confirmed_augments);
    let _ = app.emit("hexbox://overlay-state", &saved);
    saved
}

#[tauri::command]
fn set_manual_augment_override(
    app: AppHandle,
    state: State<'_, AppState>,
    slot: usize,
    augment_name: String,
) -> Result<OverlayStatePayload, String> {
    let saved = state
        .core
        .set_manual_augment_override(slot, augment_name)
        .map_err(|e| e.to_string())?;
    let _ = app.emit("hexbox://overlay-state", &saved);
    Ok(saved)
}

#[tauri::command]
fn get_augment_offer_stats(
    state: State<'_, AppState>,
    top_n: Option<usize>,
) -> AugmentOfferStatsReport {
    state.core.get_augment_offer_stats(top_n.unwrap_or(30))
}

#[tauri::command]
async fn grab_bench_champion(
    state: State<'_, AppState>,
    champion_id: u32,
) -> Result<BenchGrabStatus, String> {
    Ok(state.core.grab_bench_champion(champion_id).await)
}

#[tauri::command]
fn cancel_bench_grab(state: State<'_, AppState>) -> BenchGrabStatus {
    state.core.cancel_bench_grab()
}

#[tauri::command]
fn get_bench_grab_status(state: State<'_, AppState>) -> BenchGrabStatus {
    state.core.get_bench_grab_status()
}

#[tauri::command]
fn show_overlay(app: AppHandle, kind: String) -> Result<(), String> {
    let label = overlay_label(&kind);
    if let Some(win) = app.get_webview_window(label) {
        win.show().map_err(|e| e.to_string())?;
        let _ = win.set_focus();
    }
    Ok(())
}

#[tauri::command]
fn hide_overlay(app: AppHandle, kind: String) -> Result<(), String> {
    let label = overlay_label(&kind);
    if let Some(win) = app.get_webview_window(label) {
        win.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn toggle_overlay(app: AppHandle, kind: String) -> Result<bool, String> {
    let label = overlay_label(&kind);
    if let Some(win) = app.get_webview_window(label) {
        let visible = win.is_visible().unwrap_or(false);
        if visible {
            win.hide().map_err(|e| e.to_string())?;
            return Ok(false);
        } else {
            win.show().map_err(|e| e.to_string())?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// Keeps a WebSocket open to the League client and wakes the poller whenever
/// the game flow, champ select or ready check changes, so the app reacts at
/// once instead of on the next 1.5 s tick. Polling stays as the fallback.
fn spawn_lcu_event_listener(wake: Arc<Notify>) {
    tauri::async_runtime::spawn(async move {
        loop {
            if let Some(creds) = lcu_client::discover_lockfile() {
                let wake_on_event = wake.clone();
                let result = lcu_client::websocket::listen(&creds, move |_event| {
                    wake_on_event.notify_one();
                })
                .await;
                if let Err(err) = result {
                    eprintln!("[lcu-ws] {err}");
                }
                // Socket closed: the client exited or restarted. Refresh now.
                wake.notify_one();
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
}

fn spawn_background_lcu_poller(app: AppHandle, core: Arc<HexBoxCore>, wake: Arc<Notify>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(1500));
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = wake.notified() => {}
            }
            let outcome = core.poll_lcu_tick().await;
            if outcome.changed {
                let _ = app.emit("hexbox://lcu-status", &outcome.status);
                let _ = app.emit("hexbox://overlay-state", &outcome.overlay_state);

                let settings = core.get_settings();
                if outcome.status.connected {
                    match outcome.status.phase.as_str() {
                        "ChampSelect" => {
                            if settings.auto_show_overlay {
                                set_overlay_visible(&app, "champ-select", true);
                            }
                            set_overlay_visible(&app, "death-hint", false);
                        }
                        "InProgress" => {
                            set_overlay_visible(&app, "champ-select", false);
                            if settings.death_shop_hint_enabled {
                                set_overlay_visible(&app, "death-hint", outcome.overlay_state.is_dead);
                            }
                        }
                        _ => {
                            set_overlay_visible(&app, "champ-select", false);
                            set_overlay_visible(&app, "death-hint", false);
                        }
                    }
                }
            }
        }
    });
}

fn register_hotkeys(app: &AppHandle, core: Arc<HexBoxCore>, settings: &AppSettings) {
    let manager = app.global_shortcut();
    let _ = manager.unregister_all();

    if let Ok(augment_sc) = settings.augment_hotkey.trim().parse::<Shortcut>() {
        let core_clone = core.clone();
        if let Err(err) = manager.on_shortcut(augment_sc, move |app_handle, _shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            let visible = toggle_overlay_internal(app_handle, "augment");
            if visible {
                let app_bg = app_handle.clone();
                let core_bg = core_clone.clone();
                tauri::async_runtime::spawn(async move {
                    let preset = core_bg.get_settings().selected_preset_key;
                    let core_ocr = core_bg.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        core_ocr.run_ocr_preset(&preset, None)
                    })
                    .await;
                    let _ = app_bg.emit("hexbox://overlay-state", core_bg.get_overlay_state());
                });
            }
        }) {
            eprintln!(
                "Failed to register augment hotkey '{}': {err}",
                settings.augment_hotkey
            );
        }
    }

    if let Ok(item_sc) = settings.item_overlay_hotkey.trim().parse::<Shortcut>() {
        if let Err(err) = manager.on_shortcut(item_sc, move |app_handle, _shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            let _ = toggle_overlay_internal(app_handle, "item");
        }) {
            eprintln!(
                "Failed to register item overlay hotkey '{}': {err}",
                settings.item_overlay_hotkey
            );
        }
    }
}

fn toggle_overlay_internal(app: &AppHandle, kind: &str) -> bool {
    let label = overlay_label(kind);
    if let Some(win) = app.get_webview_window(label) {
        let visible = win.is_visible().unwrap_or(false);
        if visible {
            let _ = win.hide();
            return false;
        } else {
            let _ = win.show();
            return true;
        }
    }
    false
}

fn spawn_background_auto_ocr_watcher(app: AppHandle, core: Arc<HexBoxCore>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(1200));
        let mut was_in_progress = false;
        let mut last_emitted_sig: Vec<String> = Vec::new();

        loop {
            interval.tick().await;
            let state = core.get_overlay_state();
            let settings = core.get_settings();
            let should_watch = state.lcu.connected
                && state.lcu.phase == "InProgress"
                && settings.auto_ocr_enabled;

            if should_watch {
                was_in_progress = true;
                let preset = settings.selected_preset_key.clone();
                let core_bg = core.clone();
                let tick_res = tokio::task::spawn_blocking(move || {
                    core_bg.ocr_watch_tick(&preset, None)
                })
                .await;

                if let Ok(Ok(resp)) = tick_res {
                    let watcher = resp.get("watcher");
                    let is_stable = watcher
                        .and_then(|w| w.get("stable"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let sig: Vec<String> = watcher
                        .and_then(|w| w.get("signature"))
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(ToString::to_string))
                                .filter(|s| !s.is_empty())
                                .collect()
                        })
                        .unwrap_or_default();

                    if is_stable && sig.len() == 3 && sig != last_emitted_sig {
                        last_emitted_sig = sig;
                        let _ = app.emit("hexbox://overlay-state", core.get_overlay_state());
                        if settings.auto_show_overlay {
                            set_overlay_visible(&app, "augment", true);
                        }
                    }
                }
            } else if was_in_progress {
                was_in_progress = false;
                last_emitted_sig.clear();
                let core_bg = core.clone();
                let _ = tokio::task::spawn_blocking(move || core_bg.reset_ocr_watcher()).await;
            }
        }
    });
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let root = resolve_app_root(app.handle());
            let core =
                Arc::new(HexBoxCore::initialize(&root).expect("Failed to initialize HexBoxCore"));
            let summary = core.runtime_summary();
            println!(
                "HexBox Desktop Host Ready (Patch {}, {} heroes, {} augments, {} mechanic links)",
                summary.patch,
                summary.hero_count,
                summary.augment_count,
                summary.mechanic_link_count
            );
            register_hotkeys(app.handle(), core.clone(), &core.get_settings());
            let lcu_wake = Arc::new(Notify::new());
            spawn_background_lcu_poller(app.handle().clone(), core.clone(), lcu_wake.clone());
            spawn_lcu_event_listener(lcu_wake);
            spawn_background_auto_ocr_watcher(app.handle().clone(), core.clone());
            let layout = Arc::new(OverlayLayoutStore::load(&core.data_dir()));
            overlay_layout::apply_zoom(app.handle(), core.get_settings().overlay_zoom);
            overlay_layout::restore_positions(app.handle(), &layout);
            let layout_flush = layout.clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(1));
                loop {
                    interval.tick().await;
                    layout_flush.flush();
                }
            });
            app.manage(AppState {
                core,
                overlay_layout: layout,
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Moved(pos) = event {
                // Windows reports (-32000, -32000) for minimised windows; never save that.
                let parked = pos.x <= -30000 || pos.y <= -30000;
                if !parked && overlay_layout::is_overlay(window.label()) {
                    if let Some(state) = window.try_state::<AppState>() {
                        state
                            .overlay_layout
                            .remember(window.label(), SavedPosition { x: pos.x, y: pos.y });
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_runtime_summary,
            get_settings,
            update_settings,
            get_overlay_state,
            set_overlay_state,
            evaluate_augment_offer,
            analyze_enemy_counter,
            evaluate_5v5_lineup,
            probe_lcu_status,
            accept_ready_check,
            bench_swap,
            get_match_history,
            get_item_recommendations,
            get_postgame_augment_review,
            run_ocr_preset,
            ocr_watch_tick,
            reset_ocr_watcher,
            show_overlay,
            hide_overlay,
            toggle_overlay,
            zoom_overlay,
            reset_overlay_zoom,
            reset_overlay_position,
            start_overlay_drag,
            get_asset_sync_status,
            sync_remote_assets,
            cleanup_ocr_cache,
            get_client_locale,
            list_player_markers,
            save_player_marker,
            delete_player_marker,
            list_player_encounters,
            get_encounter_summary,
            delete_player_encounters,
            prune_player_encounters,
            export_player_library,
            import_player_library,
            search_player,
            get_player_profile,
            get_game_players,
            refresh_game_player_intel,
            get_intel_cache_status,
            get_summoner_spell_snapshot,
            apply_summoner_spells,
            sync_item_set_to_client,
            get_arurf_recommendation,
            apply_arurf_rune_page,
            set_manual_augment_selection,
            set_manual_augment_override,
            get_augment_offer_stats,
            grab_bench_champion,
            cancel_bench_grab,
            get_bench_grab_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running HexBox Tauri application");
}
