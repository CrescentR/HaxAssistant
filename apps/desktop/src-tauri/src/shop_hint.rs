//! In-game shopping panel (`death-hint-overlay`) and its launcher button
//! (`item-launcher-overlay`).
//!
//! The panel opens by itself on death and closes again on respawn. While a
//! valid game runs, the launcher stays on screen so the player can open or
//! close the panel at any time. A click is an explicit choice that the
//! poller's life-state updates (gold, items, respawn) never undo; the next
//! death re-arms the automatic behaviour unless the panel is already pinned
//! open. Leaving the game, or turning the feature off, forgets the choice.

use std::sync::Mutex;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Visibility {
    pub launcher: bool,
    pub panel: bool,
}

#[derive(Debug, Default)]
pub struct ShopHint {
    enabled: bool,
    in_game: bool,
    dead: bool,
    /// `Some` once the player clicked during this game: `true` pinned open,
    /// `false` closed.
    manual: Option<bool>,
}

impl ShopHint {
    fn active(&self) -> bool {
        self.enabled && self.in_game
    }

    /// Feed the latest game state; returns what should be on screen.
    pub fn observe(&mut self, enabled: bool, in_game: bool, dead: bool) -> Visibility {
        let died = dead && !(self.active() && self.dead);
        if !(enabled && in_game) {
            self.manual = None;
        } else if died && self.manual != Some(true) {
            self.manual = None;
        }
        self.enabled = enabled;
        self.in_game = in_game;
        self.dead = dead;
        self.visibility()
    }

    /// The feature switch changed; the game state is unchanged.
    pub fn set_enabled(&mut self, enabled: bool) -> Visibility {
        self.observe(enabled, self.in_game, self.dead)
    }

    /// Launcher click. Outside a valid game nothing changes.
    pub fn toggle(&mut self) -> Visibility {
        let open = !self.visibility().panel;
        self.set_panel(open)
    }

    /// Explicit open or close. Outside a valid game nothing changes.
    pub fn set_panel(&mut self, open: bool) -> Visibility {
        if self.active() {
            self.manual = Some(open);
        }
        self.visibility()
    }

    pub fn visibility(&self) -> Visibility {
        let active = self.active();
        Visibility {
            launcher: active,
            panel: active && self.manual.unwrap_or(self.dead),
        }
    }
}

/// Managed tauri state shared by the poller and the commands.
#[derive(Default)]
pub struct ShopHintCell(pub Mutex<ShopHint>);

impl ShopHintCell {
    pub fn with<R>(&self, f: impl FnOnce(&mut ShopHint) -> R) -> R {
        let mut guard = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }
}

/// League is in a match the in-game overlays may show for: live data is
/// flowing and, when the client is reachable, its phase is the game itself.
pub fn in_valid_game(live_game_ready: bool, lcu_connected: bool, phase: &str) -> bool {
    live_game_ready && (!lcu_connected || matches!(phase, "InProgress" | "Reconnect"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HIDDEN: Visibility = Visibility { launcher: false, panel: false };
    const BUTTON: Visibility = Visibility { launcher: true, panel: false };
    const OPEN: Visibility = Visibility { launcher: true, panel: true };

    #[test]
    fn death_shows_the_panel_and_respawn_hides_it() {
        let mut s = ShopHint::default();
        assert_eq!(s.observe(true, true, false), BUTTON);
        assert_eq!(s.observe(true, true, true), OPEN);
        assert_eq!(s.observe(true, true, true), OPEN);
        assert_eq!(s.observe(true, true, false), BUTTON);
    }

    #[test]
    fn after_respawn_the_button_opens_and_closes_the_panel() {
        let mut s = ShopHint::default();
        s.observe(true, true, true);
        assert_eq!(s.observe(true, true, false), BUTTON);
        assert_eq!(s.toggle(), OPEN);
        // Gold and item ticks while alive must not close it.
        assert_eq!(s.observe(true, true, false), OPEN);
        assert_eq!(s.observe(true, true, false), OPEN);
        assert_eq!(s.toggle(), BUTTON);
        assert_eq!(s.observe(true, true, false), BUTTON);
    }

    #[test]
    fn a_panel_pinned_while_dead_survives_respawn_and_can_still_close() {
        let mut s = ShopHint::default();
        s.observe(true, true, true);
        assert_eq!(s.toggle(), BUTTON); // closed by the player
        assert_eq!(s.observe(true, true, true), BUTTON); // death ticks don't reopen
        assert_eq!(s.toggle(), OPEN); // pinned open
        assert_eq!(s.observe(true, true, false), OPEN); // respawn keeps it
        assert_eq!(s.toggle(), BUTTON);
    }

    #[test]
    fn the_next_death_reopens_a_panel_the_player_closed() {
        let mut s = ShopHint::default();
        s.observe(true, true, true);
        s.toggle();
        s.observe(true, true, false);
        assert_eq!(s.observe(true, true, true), OPEN);
        assert_eq!(s.observe(true, true, false), BUTTON);
    }

    #[test]
    fn a_panel_pinned_open_stays_open_through_the_next_death() {
        let mut s = ShopHint::default();
        s.observe(true, true, false);
        s.toggle();
        assert_eq!(s.observe(true, true, true), OPEN);
        assert_eq!(s.observe(true, true, false), OPEN);
    }

    #[test]
    fn leaving_the_game_hides_everything_and_forgets_the_choice() {
        let mut s = ShopHint::default();
        s.observe(true, true, false);
        s.toggle();
        assert_eq!(s.observe(true, false, false), HIDDEN);
        assert_eq!(s.toggle(), HIDDEN); // no-op outside a game
        assert_eq!(s.observe(true, true, false), BUTTON);
    }

    #[test]
    fn the_feature_switch_hides_and_restores_the_button() {
        let mut s = ShopHint::default();
        s.observe(true, true, false);
        s.toggle();
        assert_eq!(s.set_enabled(false), HIDDEN);
        assert_eq!(s.toggle(), HIDDEN);
        assert_eq!(s.set_enabled(true), BUTTON);
        assert_eq!(s.observe(false, true, true), HIDDEN);
        assert_eq!(s.set_enabled(true), OPEN); // still dead: auto-show applies
    }

    #[test]
    fn explicit_show_and_hide_are_manual_choices() {
        let mut s = ShopHint::default();
        s.observe(true, true, true);
        assert_eq!(s.set_panel(false), BUTTON);
        assert_eq!(s.observe(true, true, true), BUTTON);
        assert_eq!(s.set_panel(true), OPEN);
        assert_eq!(s.observe(true, true, false), OPEN);
        assert_eq!(s.observe(true, false, false), HIDDEN);
        assert_eq!(s.set_panel(true), HIDDEN);
    }

    #[test]
    fn only_a_live_match_counts_as_a_game() {
        assert!(in_valid_game(true, true, "InProgress"));
        assert!(in_valid_game(true, true, "Reconnect"));
        assert!(in_valid_game(true, false, "Disconnected"));
        assert!(!in_valid_game(true, true, "EndOfGame"));
        assert!(!in_valid_game(true, true, "ChampSelect"));
        assert!(!in_valid_game(false, true, "InProgress"));
    }
}
