//! Keep in-game overlay windows from taking the foreground from League.
//!
//! On Windows two things activated an overlay and pulled League out of the
//! foreground (a fullscreen game then minimises):
//!
//! 1. Showing it. tao's `show()` ends in `ShowWindow(SW_SHOW)`, which
//!    activates the window. The `"focus": false` config does not prevent this:
//!    tao clears its don't-focus marker straight after creating the window, so
//!    it only applies to a window that is created visible. Our overlays are
//!    created hidden, so every later `show()` (auto-OCR offer, phase change,
//!    hotkey, toggle) took the foreground. The OCR sidecar's foreground guard
//!    then saw the overlay instead of the game, the watcher cleared the offer
//!    and hid the overlay, and the cycle repeated.
//! 2. Clicking it, e.g. "标记已选" on an augment card.
//!
//! Fixes, Windows only:
//!
//! - Overlays are shown with `ShowWindowAsync(SW_SHOWNOACTIVATE)` and re-placed
//!   at the top of the topmost band with `SWP_NOACTIVATE`; hidden with
//!   `SW_HIDE`. Both are posted to the window's own thread, so they are safe
//!   from any thread and keep their order.
//! - Overlays are made non-focusable at start-up (`WS_EX_NOACTIVATE` via tao's
//!   `set_focusable(false)`). Clicks still reach the page, so marking a card
//!   keeps working, but the system does not activate the window for them.
//!   Overlays have no text inputs, so they never need keyboard focus.
//!
//! Every overlay show and hide must go through `set_visible`: tao's own
//! visibility flag stays "hidden", so mixing in `WebviewWindow::show()` or
//! `hide()` would leave the two out of step. `is_visible()` still reports the
//! real state, because tao reads it from `IsWindowVisible`.
//!
//! Other platforms keep their previous behaviour.

#[cfg(windows)]
use tauri::Manager;
use tauri::{AppHandle, WebviewWindow};

#[cfg(windows)]
mod win32 {
    use std::ffi::c_void;

    pub const SW_HIDE: i32 = 0;
    pub const SW_SHOWNOACTIVATE: i32 = 4;
    pub const SWP_NOSIZE: u32 = 0x0001;
    pub const SWP_NOMOVE: u32 = 0x0002;
    pub const SWP_NOACTIVATE: u32 = 0x0010;
    pub const SWP_NOOWNERZORDER: u32 = 0x0200;
    pub const SWP_ASYNCWINDOWPOS: u32 = 0x4000;
    const HWND_TOPMOST: isize = -1;

    #[link(name = "user32")]
    extern "system" {
        fn ShowWindowAsync(hwnd: *mut c_void, cmd_show: i32) -> i32;
        fn SetWindowPos(
            hwnd: *mut c_void,
            insert_after: *mut c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
    }

    pub const RAISE_FLAGS: u32 =
        SWP_ASYNCWINDOWPOS | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER;

    pub fn set_visible_no_activate(hwnd: *mut c_void, visible: bool) {
        // SAFETY: `hwnd` comes from a live tauri window; both calls only post
        // messages to that window's thread and never take ownership of it.
        unsafe {
            if visible {
                ShowWindowAsync(hwnd, SW_SHOWNOACTIVATE);
                // Back on top of the topmost band, still without activating.
                SetWindowPos(hwnd, HWND_TOPMOST as *mut c_void, 0, 0, 0, 0, RAISE_FLAGS);
            } else {
                ShowWindowAsync(hwnd, SW_HIDE);
            }
        }
    }
}

/// Make every overlay window non-activating. Call once at start-up, while the
/// overlays are still hidden.
pub fn make_non_activating(app: &AppHandle) {
    #[cfg(windows)]
    for (label, _, _) in crate::overlay_layout::OVERLAYS {
        if let Some(win) = app.get_webview_window(label) {
            if let Err(err) = win.set_focusable(false) {
                eprintln!("Failed to make overlay '{label}' non-activating: {err}");
            }
        }
    }
    #[cfg(not(windows))]
    let _ = app;
}

/// Show or hide an overlay window. Showing never activates it.
#[cfg(windows)]
pub fn set_visible(win: &WebviewWindow, visible: bool) -> tauri::Result<()> {
    let hwnd = win.hwnd()?;
    win32::set_visible_no_activate(hwnd.0, visible);
    Ok(())
}

/// Show or hide an overlay window.
#[cfg(not(windows))]
pub fn set_visible(win: &WebviewWindow, visible: bool) -> tauri::Result<()> {
    if visible {
        win.show()
    } else {
        win.hide()
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::win32::*;

    #[test]
    fn show_and_raise_never_ask_for_activation() {
        // SW_SHOW (5) and SW_SHOWNORMAL (1) activate; SW_SHOWNOACTIVATE is 4.
        assert_eq!(SW_SHOWNOACTIVATE, 4);
        assert_eq!(SW_HIDE, 0);
        assert_ne!(RAISE_FLAGS & SWP_NOACTIVATE, 0);
        assert_ne!(RAISE_FLAGS & SWP_ASYNCWINDOWPOS, 0);
    }
}
