#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri v2 Desktop Host Entrypoint (`apps/desktop/src-tauri/src/main.rs`)
//! Bridges `app-core` (`HexBoxCore`), `reco-engine`, `lcu-client`, and `ocr-sidecar` to the React frontend.

use std::path::PathBuf;
use std::sync::Arc;

use app_core::HexBoxCore;
use serde_json::Value;
use shared_types::{
    AugmentRecommendationCard, EnemyCounterAnalysis, LcuStatus, LineupEvaluation, RuntimeSummary,
};
use tauri::{AppHandle, Manager, State};

struct AppState {
    core: Arc<HexBoxCore>,
}

fn resolve_app_root(app: &AppHandle) -> PathBuf {
    if let Ok(res_dir) = app.path().resource_dir() {
        if res_dir.join("data").exists() {
            return res_dir;
        }
    }
    let manifest_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();
    if manifest_root.join("data").exists() {
        return manifest_root;
    }
    std::env::current_dir().unwrap_or(manifest_root)
}

#[tauri::command]
fn get_runtime_summary(state: State<'_, AppState>) -> RuntimeSummary {
    state.core.runtime_summary()
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

#[tauri::command]
async fn probe_lcu_status(state: State<'_, AppState>) -> Result<LcuStatus, String> {
    Ok(state.core.probe_lcu().await)
}

#[tauri::command]
async fn run_ocr_preset(
    state: State<'_, AppState>,
    preset_key: Option<String>,
    image_path: Option<String>,
) -> Result<Value, String> {
    let core = state.core.clone();
    let preset = preset_key.unwrap_or_else(|| "1920x1080@100".to_string());
    tokio::task::spawn_blocking(move || core.run_ocr_preset(&preset, image_path.as_deref()))
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

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let root = resolve_app_root(app.handle());
            let core = HexBoxCore::initialize(&root)
                .expect("Failed to initialize HexBoxCore");
            let summary = core.runtime_summary();
            println!(
                "HexBox Desktop Host Ready (Patch {}, {} heroes, {} augments, {} mechanic links)",
                summary.patch,
                summary.hero_count,
                summary.augment_count,
                summary.mechanic_link_count
            );
            app.manage(AppState {
                core: Arc::new(core),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_runtime_summary,
            evaluate_augment_offer,
            analyze_enemy_counter,
            evaluate_5v5_lineup,
            probe_lcu_status,
            run_ocr_preset,
            show_overlay,
            hide_overlay,
            toggle_overlay,
        ])
        .run(tauri::generate_context!())
        .expect("error while running HexBox Tauri application");
}
