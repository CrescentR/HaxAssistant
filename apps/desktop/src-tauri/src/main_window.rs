//! Main window behaviour: remembered size/position (`main_window.json`), the
//! close-button action (ask / minimise to tray / exit) and the tray icon.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

pub const MAIN_LABEL: &str = "main";
/// Smallest size worth restoring; mirrors `minWidth`/`minHeight` in `tauri.conf.json`.
const MIN_WIDTH: u32 = 1024;
const MIN_HEIGHT: u32 = 680;

pub const CLOSE_ASK: &str = "ask";
pub const CLOSE_TRAY: &str = "tray";
pub const CLOSE_EXIT: &str = "exit";

/// Normalise a stored `close_button_action`; unknown values fall back to asking.
pub fn close_action(raw: &str) -> &'static str {
    match raw {
        CLOSE_TRAY => CLOSE_TRAY,
        CLOSE_EXIT => CLOSE_EXIT,
        _ => CLOSE_ASK,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct MainWindowGeometry {
    pub width: u32,
    pub height: u32,
    pub x: Option<i32>,
    pub y: Option<i32>,
    #[serde(default)]
    pub maximized: bool,
}

impl MainWindowGeometry {
    fn usable_size(&self) -> Option<(u32, u32)> {
        (self.width >= MIN_WIDTH && self.height >= MIN_HEIGHT).then_some((self.width, self.height))
    }
}

#[derive(Default)]
struct Inner {
    geometry: Option<MainWindowGeometry>,
    dirty: bool,
}

pub struct MainWindowStore {
    path: PathBuf,
    inner: Mutex<Inner>,
}

impl MainWindowStore {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("main_window.json");
        let geometry = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok());
        Self {
            path,
            inner: Mutex::new(Inner {
                geometry,
                dirty: false,
            }),
        }
    }

    pub fn get(&self) -> Option<MainWindowGeometry> {
        self.inner.lock().ok()?.geometry
    }

    fn update(&self, f: impl FnOnce(&mut MainWindowGeometry)) {
        let Ok(mut inner) = self.inner.lock() else { return };
        let mut next = inner.geometry.unwrap_or_default();
        f(&mut next);
        if inner.geometry != Some(next) {
            inner.geometry = Some(next);
            inner.dirty = true;
        }
    }

    /// Record the window's current state. While maximised only the flag is
    /// stored, so un-maximising later returns to the last normal size.
    pub fn capture(&self, win: &WebviewWindow) {
        let maximized = win.is_maximized().unwrap_or(false);
        let minimized = win.is_minimized().unwrap_or(false);
        if minimized || !win.is_visible().unwrap_or(true) {
            return;
        }
        let size = win.inner_size().ok();
        let pos = win.outer_position().ok();
        self.update(|g| {
            g.maximized = maximized;
            if maximized {
                return;
            }
            if let Some(size) = size {
                g.width = size.width;
                g.height = size.height;
            }
            if let Some(pos) = pos {
                // Windows reports (-32000, -32000) for minimised windows.
                if pos.x > -30000 && pos.y > -30000 {
                    g.x = Some(pos.x);
                    g.y = Some(pos.y);
                }
            }
        });
    }

    pub fn flush(&self) {
        let Ok(mut inner) = self.inner.lock() else { return };
        if !inner.dirty {
            return;
        }
        let Some(geometry) = inner.geometry else { return };
        if let Ok(raw) = serde_json::to_string_pretty(&geometry) {
            if fs::write(&self.path, raw).is_ok() {
                inner.dirty = false;
            }
        }
    }
}

fn position_on_screen(win: &WebviewWindow, x: i32, y: i32) -> bool {
    let Ok(monitors) = win.available_monitors() else { return true };
    // Require the title bar area (top-left + 100 px) to be visible somewhere.
    monitors.iter().any(|m| {
        let o = m.position();
        let s = m.size();
        x + 100 >= o.x && y >= o.y && x + 100 < o.x + s.width as i32 && y + 40 < o.y + s.height as i32
    })
}

pub fn restore(app: &AppHandle, store: &MainWindowStore) {
    let (Some(win), Some(g)) = (app.get_webview_window(MAIN_LABEL), store.get()) else {
        return;
    };
    if let Some((w, h)) = g.usable_size() {
        let _ = win.set_size(PhysicalSize::new(w, h));
    }
    if let (Some(x), Some(y)) = (g.x, g.y) {
        if position_on_screen(&win, x, y) {
            let _ = win.set_position(PhysicalPosition::new(x, y));
        }
    }
    if g.maximized {
        let _ = win.maximize();
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(MAIN_LABEL) {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

pub fn hide_to_tray(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(MAIN_LABEL) {
        let _ = win.hide();
    }
}

/// Tray icon: left click shows the main window; menu has Show / Exit.
pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "tray-show", "显示主窗口", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray-quit", "退出 HaxAssistant", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut builder = TrayIconBuilder::with_id("haxassistant-tray")
        .tooltip("HaxAssistant · 海克斯助手")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray-show" => show_main(app),
            "tray-quit" => crate::exit_app(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_close_action_falls_back_to_ask() {
        assert_eq!(close_action("tray"), CLOSE_TRAY);
        assert_eq!(close_action("exit"), CLOSE_EXIT);
        assert_eq!(close_action(""), CLOSE_ASK);
        assert_eq!(close_action("minimize"), CLOSE_ASK);
    }

    #[test]
    fn geometry_survives_reload_and_rejects_tiny_sizes() {
        let dir = std::env::temp_dir().join(format!("hexbox-main-window-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let store = MainWindowStore::load(&dir);
        assert_eq!(store.get(), None);
        store.update(|g| {
            g.width = 1400;
            g.height = 900;
            g.x = Some(40);
            g.y = Some(60);
        });
        store.flush();

        let reloaded = MainWindowStore::load(&dir).get().unwrap();
        assert_eq!(reloaded.usable_size(), Some((1400, 900)));
        assert_eq!((reloaded.x, reloaded.y), (Some(40), Some(60)));

        let tiny = MainWindowGeometry {
            width: 300,
            height: 200,
            ..Default::default()
        };
        assert_eq!(tiny.usable_size(), None);

        let _ = fs::remove_dir_all(&dir);
    }
}
