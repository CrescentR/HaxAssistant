#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri v2 Desktop Host Entrypoint (`apps/desktop/src-tauri/src/main.rs`)
//! Bridges `app-core` (`HexBoxCore`), `reco-engine`, `lcu-client`, and persistent `ocr-sidecar`
//! to the React frontend and overlay windows.

mod main_window;
mod overlay_layout;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use app_core::HexBoxCore;
use serde_json::Value;
use shared_types::{
    AppSettings, AppUpdateInfo, AppUpdateProgress, ArurfRecommendation, AssetSyncResult,
    AssetSyncStatus, AugmentOfferStatsReport,
    AugmentRecommendationCard, BenchGrabStatus, DataUpdateOutcome, EnemyCounterAnalysis,
    GamePlayerIntel, IntelCacheStatus, ItemPlan, ItemSetSyncResult, LcuStatus, LeagueWindowMode,
    LineupEvaluation, MatchHistoryEntry, MatchHistoryResponse, OcrCacheCleanupReport,
    OverlayStatePayload, PlayerEncounterRecord, PlayerEncounterSummary, PlayerLibraryBundle,
    PlayerMarker, PlayerProfile, PlayerSearchResult, PostgameReview, RunePagePlan, RuntimeSummary,
    SummonerSpellSnapshot,
};
use main_window::MainWindowStore;
use overlay_layout::{OverlayLayoutStore, SavedPosition};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow, WindowEvent};
use tokio::sync::Notify;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

struct AppState {
    core: Arc<HexBoxCore>,
    overlay_layout: Arc<OverlayLayoutStore>,
    main_window: Arc<MainWindowStore>,
}

/// Save window layouts and quit.
pub(crate) fn exit_app(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.overlay_layout.flush();
        if let Some(win) = app.get_webview_window(main_window::MAIN_LABEL) {
            state.main_window.capture(&win);
        }
        state.main_window.flush();
    }
    app.exit(0);
}

/// Carry out what the main window's × should do.
fn perform_close_action(app: &AppHandle, action: &str) {
    match main_window::close_action(action) {
        main_window::CLOSE_TRAY => main_window::hide_to_tray(app),
        main_window::CLOSE_EXIT => exit_app(app),
        _ => {
            // Let the UI ask; it answers through `handle_main_close`.
            main_window::show_main(app);
            let _ = app.emit("haxassistant://close-requested", ());
        }
    }
}

/// Answer to the close prompt: `tray` or `exit`; `remember` stores the choice.
#[tauri::command]
fn handle_main_close(
    app: AppHandle,
    state: State<'_, AppState>,
    action: String,
    remember: bool,
) -> Result<(), String> {
    let action = main_window::close_action(&action);
    if action == main_window::CLOSE_ASK {
        return Err("action must be tray or exit".to_string());
    }
    if remember {
        let mut settings = state.core.get_settings();
        settings.close_button_action = action.to_string();
        let saved = state
            .core
            .update_settings(settings)
            .map_err(|e| e.to_string())?;
        let _ = app.emit("haxassistant://settings-updated", &saved);
    }
    perform_close_action(&app, action);
    Ok(())
}

fn resolve_app_roots(app: &AppHandle) -> (PathBuf, PathBuf) {
    let has_data = |dir: &PathBuf| {
        dir.join("data/champions.json").exists()
            || dir.join("data/runtime_meta.json").exists()
            || dir.join("resources/data/champions.json").exists()
            || dir.join("resources/data/runtime_meta.json").exists()
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
    let resource_dir = app.path().resource_dir().ok();
    let local_data_dir = app.path().app_local_data_dir().ok();

    // The source checkout wins in debug builds so dev runs use the live data
    // and the sidecar's virtualenv instead of the copies staged for bundling.
    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(debug_assertions) {
        candidates.push(manifest_root.clone());
    }
    if let Some(dir) = &exe_dir {
        candidates.push(dir.clone());
    }
    if let Some(dir) = &resource_dir {
        candidates.push(dir.clone());
    }
    if let Some(dir) = &local_data_dir {
        candidates.push(dir.clone());
    }
    if cfg!(debug_assertions) {
        candidates.push(manifest_root.clone());
    }
    candidates.extend(std::env::current_dir().ok());

    let fallback = if cfg!(debug_assertions) {
        manifest_root
    } else {
        resource_dir
            .or(exe_dir)
            .or_else(|| local_data_dir.clone())
            .unwrap_or(manifest_root)
    };

    let resolved = candidates.into_iter().find(has_data).unwrap_or(fallback);
    let data_root = if cfg!(debug_assertions) {
        resolved.clone()
    } else {
        app_core::prepare_writable_data_root(&resolved, local_data_dir.as_deref())
    };
    (data_root, resolved)
}

#[tauri::command]
async fn read_data_file(state: State<'_, AppState>, file_name: String) -> Result<String, String> {
    let trimmed = file_name.trim();
    let valid_name = !trimmed.is_empty()
        && trimmed.ends_with(".json")
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
        && !trimmed.contains("..");
    if !valid_name {
        return Err(format!("Invalid data file name: {file_name}"));
    }
    let path = state.core.data_dir().join(trimmed);
    tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))
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
    let _ = app.emit("haxassistant://settings-updated", &saved);
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
    let _ = app.emit("haxassistant://settings-updated", &saved);
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
fn confirm_ocr_augment(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<OverlayStatePayload, String> {
    let saved = state.core.confirm_ocr_augment(&name).map_err(|err| err.to_string())?;
    let _ = app.emit("haxassistant://overlay-state", &saved);
    set_overlay_visible(&app, "augment", false);
    Ok(saved)
}

#[tauri::command]
fn set_augment_stage(app: AppHandle, state: State<'_, AppState>, stage: u8) -> Result<OverlayStatePayload, String> {
    let saved = state.core.set_augment_stage(stage).map_err(|err| err.to_string())?;
    let _ = app.emit("haxassistant://overlay-state", &saved);
    Ok(saved)
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
    let _ = app.emit("haxassistant://overlay-state", &overlay_state);
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
    let _ = app.emit("haxassistant://overlay-state", &overlay_state);
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
    app: AppHandle,
    state: State<'_, AppState>,
    source_url: Option<String>,
    include_models: Option<bool>,
) -> Result<AssetSyncResult, String> {
    let result = state
        .core
        .sync_remote_assets(source_url.as_deref(), include_models.unwrap_or(true))
        .await
        .map_err(|e| e.to_string())?;
    if !result.downloaded_data_files.is_empty() {
        let _ = app.emit("haxassistant://dataset-updated", state.core.runtime_summary());
    }
    Ok(result)
}

#[tauri::command]
async fn sync_runtime_bundle(
    app: AppHandle,
    state: State<'_, AppState>,
    source_url: Option<String>,
    apply: Option<bool>,
) -> Result<DataUpdateOutcome, String> {
    let out = state
        .core
        .sync_runtime_bundle(source_url.as_deref(), apply.unwrap_or(true))
        .await
        .map_err(|e| e.to_string())?;
    if out.status == "updated" {
        let _ = app.emit("haxassistant://dataset-updated", state.core.runtime_summary());
    }
    Ok(out)
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
async fn get_game_window_mode(state: State<'_, AppState>) -> Result<LeagueWindowMode, String> {
    Ok(state.core.get_game_window_mode().await)
}

#[tauri::command]
async fn fix_game_borderless(state: State<'_, AppState>) -> Result<LeagueWindowMode, String> {
    state
        .core
        .fix_game_borderless()
        .await
        .map_err(|e| e.to_string())
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
    let _ = app.emit("haxassistant://overlay-state", &saved);
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
    let _ = app.emit("haxassistant://overlay-state", &saved);
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
                // Socket closed: the client exited or restarted. Invalidate cache and refresh now.
                lcu_client::invalidate_discovery_cache();
                wake.notify_one();
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
}

fn spawn_background_lcu_poller(
    app: AppHandle,
    core: Arc<HexBoxCore>,
    wake: Arc<Notify>,
    data_update_wake: Arc<Notify>,
) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(1500));
        let mut was_safe_to_swap = true;
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = wake.notified() => {}
            }
            let outcome = core.poll_lcu_tick().await;
            let safe_now = core.is_safe_to_swap_data();
            if !was_safe_to_swap && safe_now {
                data_update_wake.notify_one();
            }
            was_safe_to_swap = safe_now;
            if outcome.changed {
                let _ = app.emit("haxassistant://lcu-status", &outcome.status);
                let _ = app.emit("haxassistant://overlay-state", &outcome.overlay_state);

                let settings = core.get_settings();
                if !outcome.overlay_state.live_game_ready {
                    set_overlay_visible(&app, "augment", false);
                    set_overlay_visible(&app, "item", false);
                    set_overlay_visible(&app, "player-scores", false);
                    set_overlay_visible(&app, "death-hint", false);
                }
                if !outcome.status.connected {
                    set_overlay_visible(&app, "champ-select", false);
                } else {
                    match outcome.status.phase.as_str() {
                        "ChampSelect" => {
                            if settings.auto_show_overlay {
                                set_overlay_visible(&app, "champ-select", true);
                            }
                            set_overlay_visible(&app, "death-hint", false);
                        }
                        "InProgress" | "Reconnect" => {
                            set_overlay_visible(&app, "champ-select", false);
                            if settings.death_shop_hint_enabled && outcome.overlay_state.live_game_ready {
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
            let overlay = app_handle.get_webview_window(overlay_label("augment"));
            if overlay.as_ref().is_some_and(|window| window.is_visible().unwrap_or(false)) {
                set_overlay_visible(app_handle, "augment", false);
                return;
            }
            if !core_clone.get_overlay_state().live_game_ready {
                return;
            }
            let app_bg = app_handle.clone();
            let core_bg = core_clone.clone();
            tauri::async_runtime::spawn(async move {
                let preset = core_bg.get_settings().selected_preset_key;
                let core_ocr = core_bg.clone();
                let result = tokio::task::spawn_blocking(move || core_ocr.run_ocr_preset(&preset, None)).await;
                if let Ok(Ok(response)) = result {
                    let state = core_bg.get_overlay_state();
                    let recognized_three = response.get("ocr_matches").and_then(Value::as_array)
                        .is_some_and(|matches| matches.len() == 3 && matches.iter().all(|entry| {
                            entry.get("match_name").and_then(Value::as_str).is_some_and(|name| !name.is_empty())
                        }));
                    if state.live_game_ready && recognized_three {
                        let _ = app_bg.emit("haxassistant://overlay-state", &state);
                        set_overlay_visible(&app_bg, "augment", true);
                    }
                }
            });
        }) {
            eprintln!(
                "Failed to register augment hotkey '{}': {err}",
                settings.augment_hotkey
            );
        }
    }

    if let Ok(item_sc) = settings.item_overlay_hotkey.trim().parse::<Shortcut>() {
        if let Err(err) = manager.on_shortcut(item_sc, move |app_handle, _shortcut, event| {
            if event.state() != ShortcutState::Pressed || !core.get_overlay_state().live_game_ready {
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
        let mut interval = tokio::time::interval(Duration::from_millis(450));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut watching = false;
        let mut misses = 0u8;
        let mut last_signature = Vec::<String>::new();

        loop {
            interval.tick().await;
            let settings = core.get_settings();
            if !settings.auto_ocr_enabled || !core.get_overlay_state().live_game_ready {
                if watching {
                    watching = false;
                    misses = 0;
                    last_signature.clear();
                    if core.clear_ocr_offer() {
                        let _ = app.emit("haxassistant://overlay-state", core.get_overlay_state());
                    }
                    set_overlay_visible(&app, "augment", false);
                    let reset = core.clone();
                    let _ = tokio::task::spawn_blocking(move || reset.reset_ocr_watcher()).await;
                }
                continue;
            }
            watching = true;
            let preset = settings.selected_preset_key;
            let worker = core.clone();
            let result = tokio::task::spawn_blocking(move || worker.ocr_watch_tick(&preset, None)).await;
            let response = result.ok().and_then(Result::ok);
            let signature: Vec<String> = response.as_ref()
                .and_then(|response| response.pointer("/watcher/signature"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect();

            if signature.len() != 3 {
                misses = misses.saturating_add(1);
                if misses >= 2 {
                    last_signature.clear();
                    if core.clear_ocr_offer() {
                        let _ = app.emit("haxassistant://overlay-state", core.get_overlay_state());
                        set_overlay_visible(&app, "augment", false);
                    }
                }
                continue;
            }
            misses = 0;
            let stable = response.as_ref()
                .and_then(|response| response.pointer("/watcher/stable"))
                .and_then(Value::as_bool) == Some(true);
            let state = core.get_overlay_state();
            if stable && state.live_game_ready && state.candidate_augments == signature && last_signature != signature {
                last_signature = signature;
                let _ = app.emit("haxassistant://overlay-state", &state);
                if settings.auto_show_overlay {
                    set_overlay_visible(&app, "augment", true);
                }
            }
        }
    });
}

async fn query_app_update(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| format!("更新组件不可用：{e}"))?;
    updater
        .check()
        .await
        .map_err(|e| format!("检查更新失败：{e}"))
}

fn update_info(app: &AppHandle, update: Option<&tauri_plugin_updater::Update>) -> AppUpdateInfo {
    AppUpdateInfo {
        available: update.is_some(),
        current_version: app.package_info().version.to_string(),
        version: update.map(|u| u.version.clone()),
        notes: update.and_then(|u| u.body.clone()),
        date: update.and_then(|u| u.date.map(|d| d.to_string())),
    }
}

#[tauri::command]
async fn check_app_update(app: AppHandle) -> Result<AppUpdateInfo, String> {
    let update = query_app_update(&app).await?;
    Ok(update_info(&app, update.as_ref()))
}

/// Download, verify (minisign) and install the latest release, then restart.
/// Progress is emitted as `haxassistant://update-progress`.
#[tauri::command]
async fn install_app_update(app: AppHandle) -> Result<(), String> {
    let Some(update) = query_app_update(&app).await? else {
        return Err("已经是最新版本".to_string());
    };
    if let Some(state) = app.try_state::<AppState>() {
        state.overlay_layout.flush();
    }
    let progress_app = app.clone();
    let finish_app = app.clone();
    let mut downloaded: u64 = 0;
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress_app.emit(
                    "haxassistant://update-progress",
                    AppUpdateProgress {
                        downloaded,
                        total,
                        finished: false,
                    },
                );
            },
            move || {
                let _ = finish_app.emit(
                    "haxassistant://update-progress",
                    AppUpdateProgress {
                        downloaded: 0,
                        total: None,
                        finished: true,
                    },
                );
            },
        )
        .await
        .map_err(|e| format!("安装更新失败：{e}"))?;
    app.restart();
}

/// Check once shortly after start (release builds only) and tell the UI.
fn spawn_startup_update_check(app: AppHandle, core: Arc<HexBoxCore>) {
    if cfg!(debug_assertions) || !core.get_settings().auto_check_update {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(8)).await;
        match query_app_update(&app).await {
            Ok(Some(update)) => {
                let _ = app.emit("haxassistant://update-available", update_info(&app, Some(&update)));
            }
            Ok(None) => {}
            Err(err) => eprintln!("{err}"),
        }
    });
}

/// Periodically checks the backend for newer runtime data bundles (every 6 hours,
/// plus once shortly after startup) and applies updates only between matches.
fn spawn_background_data_updater(
    app: AppHandle,
    core: Arc<HexBoxCore>,
    wake: Arc<Notify>,
) {
    const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 3600);
    const RETRY_INTERVAL: Duration = Duration::from_secs(15 * 60);

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(6)).await;
        let mut pending_deferred = false;
        let mut retry_delay = RETRY_INTERVAL;
        let mut next_check_at = tokio::time::Instant::now();

        loop {
            let configured_url = if cfg!(debug_assertions) {
                let custom = core.get_settings().data_sync_url;
                let from_env = std::env::var("HAXASSISTANT_DATA_SYNC_URL").unwrap_or_default();
                if !custom.trim().is_empty() || !from_env.trim().is_empty() {
                    core.resolve_data_sync_url(None)
                } else {
                    String::new()
                }
            } else {
                core.resolve_data_sync_url(None)
            };

            let now = tokio::time::Instant::now();
            let due_deferred = pending_deferred && core.is_safe_to_swap_data();
            let due_scheduled = now >= next_check_at;

            if !configured_url.is_empty() && (due_deferred || due_scheduled) {
                let apply = core.is_safe_to_swap_data();
                match core.sync_runtime_bundle(None, apply).await {
                    Ok(out) => {
                        retry_delay = RETRY_INTERVAL;
                        match out.status.as_str() {
                            "updated" => {
                                println!("[data-updater] {}", out.message);
                                pending_deferred = false;
                                next_check_at = tokio::time::Instant::now() + CHECK_INTERVAL;
                                let _ = app.emit(
                                    "haxassistant://dataset-updated",
                                    core.runtime_summary(),
                                );
                            }
                            "available" => {
                                pending_deferred = true;
                                next_check_at = tokio::time::Instant::now() + CHECK_INTERVAL;
                            }
                            _ => {
                                pending_deferred = false;
                                next_check_at = tokio::time::Instant::now() + CHECK_INTERVAL;
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("[data-updater] 自动检查数据更新失败: {err}");
                        pending_deferred = false;
                        next_check_at = tokio::time::Instant::now() + retry_delay;
                        retry_delay = (retry_delay * 2).min(CHECK_INTERVAL);
                    }
                }
            } else if configured_url.is_empty() {
                next_check_at = tokio::time::Instant::now() + CHECK_INTERVAL;
            }

            let sleep_until = next_check_at.max(tokio::time::Instant::now() + Duration::from_secs(5));
            tokio::select! {
                _ = tokio::time::sleep_until(sleep_until) => {}
                _ = wake.notified() => {}
            }
        }
    });
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let (root, resource_root) = resolve_app_roots(app.handle());
            let core =
                Arc::new(HexBoxCore::initialize_with_resources(&root, &resource_root).expect("Failed to initialize HexBoxCore"));
            let summary = core.runtime_summary();
            println!(
                "HaxAssistant Desktop Host Ready (Patch {}, {} heroes, {} augments, {} mechanic links)",
                summary.patch,
                summary.hero_count,
                summary.augment_count,
                summary.mechanic_link_count
            );
            register_hotkeys(app.handle(), core.clone(), &core.get_settings());
            let lcu_wake = Arc::new(Notify::new());
            let data_update_wake = Arc::new(Notify::new());
            spawn_background_lcu_poller(
                app.handle().clone(),
                core.clone(),
                lcu_wake.clone(),
                data_update_wake.clone(),
            );
            spawn_lcu_event_listener(lcu_wake);
            spawn_background_auto_ocr_watcher(app.handle().clone(), core.clone());
            spawn_startup_update_check(app.handle().clone(), core.clone());
            spawn_background_data_updater(app.handle().clone(), core.clone(), data_update_wake);
            let layout = Arc::new(OverlayLayoutStore::load(&core.data_dir()));
            overlay_layout::apply_zoom(app.handle(), core.get_settings().overlay_zoom);
            overlay_layout::restore_positions(app.handle(), &layout);
            let main_store = Arc::new(MainWindowStore::load(&core.data_dir()));
            main_window::restore(app.handle(), &main_store);
            if let Err(err) = main_window::build_tray(app.handle()) {
                eprintln!("Failed to create tray icon: {err}");
            }
            let layout_flush = layout.clone();
            let main_flush = main_store.clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(1));
                loop {
                    interval.tick().await;
                    layout_flush.flush();
                    main_flush.flush();
                }
            });
            app.manage(AppState {
                core,
                overlay_layout: layout,
                main_window: main_store,
            });
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_)
                if window.label() == main_window::MAIN_LABEL =>
            {
                if let (Some(state), Some(win)) = (
                    window.try_state::<AppState>(),
                    window.app_handle().get_webview_window(main_window::MAIN_LABEL),
                ) {
                    state.main_window.capture(&win);
                }
            }
            WindowEvent::Moved(pos) => {
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
            WindowEvent::CloseRequested { api, .. } if window.label() == main_window::MAIN_LABEL => {
                api.prevent_close();
                let app = window.app_handle();
                let action = app
                    .try_state::<AppState>()
                    .map(|s| s.core.get_settings().close_button_action)
                    .unwrap_or_default();
                perform_close_action(app, &action);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_runtime_summary,
            read_data_file,
            get_settings,
            update_settings,
            get_overlay_state,
            confirm_ocr_augment,
            set_augment_stage,
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
            sync_runtime_bundle,
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
            check_app_update,
            install_app_update,
            handle_main_close,
            get_game_window_mode,
            fix_game_borderless,
        ])
        .run(tauri::generate_context!())
        .expect("error while running HexBox Tauri application");
}
