//! Overlay window layout: window size follows the zoom setting, and each
//! overlay remembers where the user dragged it (`overlay_layout.json`).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewWindow};

pub const ZOOM_MIN: u16 = 75;
pub const ZOOM_MAX: u16 = 130;
pub const ZOOM_DEFAULT: u16 = 100;
pub const ZOOM_STEP: i32 = 5;

/// (window label, base width, base height) at 100 % zoom; mirrors `tauri.conf.json`.
pub const OVERLAYS: [(&str, f64, f64); 6] = [
    ("augment-overlay", 980.0, 340.0),
    ("formula-item-overlay", 460.0, 280.0),
    ("champ-select-overlay", 420.0, 540.0),
    ("player-scores-overlay", 640.0, 240.0),
    ("death-hint-overlay", 380.0, 140.0),
    (LAUNCHER_LABEL, 44.0, 44.0),
];

/// The small in-game button that opens the shopping panel. Its window is the
/// button itself, so it takes no clicks outside its own few pixels.
pub const LAUNCHER_LABEL: &str = "item-launcher-overlay";

pub fn is_overlay(label: &str) -> bool {
    OVERLAYS.iter().any(|(l, _, _)| *l == label)
}

pub fn clamp_zoom(zoom: i32) -> u16 {
    zoom.clamp(ZOOM_MIN as i32, ZOOM_MAX as i32) as u16
}

/// Window size for an overlay at the given zoom percentage.
pub fn scaled_size(base: (f64, f64), zoom: u16) -> (f64, f64) {
    let factor = clamp_zoom(zoom as i32) as f64 / 100.0;
    ((base.0 * factor).round(), (base.1 * factor).round())
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SavedPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Default)]
struct Inner {
    positions: HashMap<String, SavedPosition>,
    dirty: bool,
}

pub struct OverlayLayoutStore {
    path: PathBuf,
    inner: Mutex<Inner>,
}

impl OverlayLayoutStore {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("overlay_layout.json");
        let positions = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        Self {
            path,
            inner: Mutex::new(Inner {
                positions,
                dirty: false,
            }),
        }
    }

    pub fn get(&self, label: &str) -> Option<SavedPosition> {
        self.inner.lock().ok()?.positions.get(label).copied()
    }

    pub fn remember(&self, label: &str, pos: SavedPosition) {
        if let Ok(mut inner) = self.inner.lock() {
            if inner.positions.get(label) != Some(&pos) {
                inner.positions.insert(label.to_string(), pos);
                inner.dirty = true;
            }
        }
    }

    pub fn forget(&self, label: Option<&str>) {
        if let Ok(mut inner) = self.inner.lock() {
            match label {
                Some(l) => {
                    inner.positions.remove(l);
                }
                None => inner.positions.clear(),
            }
            inner.dirty = true;
        }
    }

    /// Write to disk if anything changed since the last flush.
    pub fn flush(&self) {
        let Ok(mut inner) = self.inner.lock() else { return };
        if !inner.dirty {
            return;
        }
        if let Ok(raw) = serde_json::to_string_pretty(&inner.positions) {
            if fs::write(&self.path, raw).is_ok() {
                inner.dirty = false;
            }
        }
    }
}

/// True when the point lies on a connected monitor, so a position saved on a
/// display that has since been unplugged does not strand the window off-screen.
fn is_on_screen(win: &WebviewWindow, pos: SavedPosition) -> bool {
    let Ok(monitors) = win.available_monitors() else { return true };
    monitors.iter().any(|m| {
        let origin = m.position();
        let size = m.size();
        pos.x >= origin.x
            && pos.y >= origin.y
            && pos.x < origin.x + size.width as i32
            && pos.y < origin.y + size.height as i32
    })
}

pub fn apply_zoom(app: &AppHandle, zoom: u16) {
    for (label, w, h) in OVERLAYS {
        if let Some(win) = app.get_webview_window(label) {
            let (width, height) = scaled_size((w, h), zoom);
            let _ = win.set_size(LogicalSize::new(width, height));
        }
    }
}

/// Where the launcher goes until the user drags it: the right edge of the
/// display the shopping panel is on (the primary one if that is unknown), a
/// little below the middle, clear of the minimap.
fn launcher_default(win: &WebviewWindow) -> Option<PhysicalPosition<i32>> {
    let panel_monitor = win
        .app_handle()
        .get_webview_window("death-hint-overlay")
        .and_then(|panel| panel.current_monitor().ok().flatten());
    let monitor = match panel_monitor {
        Some(m) => m,
        None => win.primary_monitor().ok().flatten()?,
    };
    let origin = monitor.position();
    let screen = monitor.size();
    let size = win.outer_size().ok()?;
    let margin = (16.0 * monitor.scale_factor()).round() as i32;
    Some(PhysicalPosition::new(
        origin.x + screen.width as i32 - size.width as i32 - margin,
        origin.y + (screen.height as f64 * 0.58).round() as i32,
    ))
}

/// Move an overlay to its default spot: centred, or the launcher's corner.
fn place_default(label: &str, win: &WebviewWindow) {
    match (label == LAUNCHER_LABEL).then(|| launcher_default(win)).flatten() {
        Some(pos) => {
            let _ = win.set_position(pos);
        }
        None => {
            let _ = win.center();
        }
    }
}

pub fn restore_positions(app: &AppHandle, store: &OverlayLayoutStore) {
    for (label, _, _) in OVERLAYS {
        let Some(win) = app.get_webview_window(label) else { continue };
        match store.get(label) {
            Some(pos) if is_on_screen(&win, pos) => {
                let _ = win.set_position(PhysicalPosition::new(pos.x, pos.y));
            }
            _ if label == LAUNCHER_LABEL => place_default(label, &win),
            _ => {}
        }
    }
}

/// Forget saved positions and re-centre the affected overlays.
pub fn reset_positions(app: &AppHandle, store: &OverlayLayoutStore, label: Option<&str>) {
    store.forget(label);
    for (l, _, _) in OVERLAYS {
        if label.is_some_and(|wanted| wanted != l) {
            continue;
        }
        if let Some(win) = app.get_webview_window(l) {
            place_default(l, &win);
        }
    }
    // Moving fires Moved events that would re-save the default spot.
    store.forget(label);
    store.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_is_clamped_and_scales_window_size() {
        assert_eq!(clamp_zoom(10), ZOOM_MIN);
        assert_eq!(clamp_zoom(500), ZOOM_MAX);
        assert_eq!(scaled_size((980.0, 340.0), 100), (980.0, 340.0));
        assert_eq!(scaled_size((980.0, 340.0), 130), (1274.0, 442.0));
        assert_eq!(scaled_size((980.0, 340.0), 999), (1274.0, 442.0));
    }

    #[test]
    fn positions_survive_a_reload_and_can_be_forgotten() {
        let dir = std::env::temp_dir().join(format!("hexbox-layout-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let store = OverlayLayoutStore::load(&dir);
        assert_eq!(store.get("augment-overlay"), None);
        store.remember("augment-overlay", SavedPosition { x: 120, y: -40 });
        store.flush();

        let reloaded = OverlayLayoutStore::load(&dir);
        assert_eq!(
            reloaded.get("augment-overlay"),
            Some(SavedPosition { x: 120, y: -40 })
        );
        reloaded.forget(Some("augment-overlay"));
        reloaded.flush();
        assert_eq!(OverlayLayoutStore::load(&dir).get("augment-overlay"), None);

        let _ = fs::remove_dir_all(&dir);
    }
}
