//! CapsLock as a language key: tap switches language, hold is a real
//! CapsLock, another key pressed during the press is a rollover.
//! Pure; times are `Instant`s passed in, no OS calls.
//!
//! Transitions (`hold` is the configured hold time):
//!
//! | state    | event                    | action             | next      |
//! |----------|--------------------------|--------------------|-----------|
//! | Idle     | down (fresh)             | None               | Down      |
//! | Idle     | down (repeat)            | None               | Idle      |
//! | Down     | down (fresh, stale)      | None               | Down(now) |
//! | Down     | down (repeat)            | None               | Down      |
//! | Down     | other key                | SwitchThenReplay   | Chorded   |
//! | Down     | hold timer, age >= hold  | ToggleCapsLock     | HoldFired |
//! | Down     | up, age < hold           | Switch             | Idle      |
//! | Down     | up, age >= hold          | ToggleCapsLock     | Idle      |
//! | Chorded  | other key / timer        | None               | Chorded   |
//! | Chorded  | up                       | None               | Idle      |
//! | HoldFired| other key / timer / up   | None               | HoldFired / Idle on up |
//! | any      | focus changed            | None               | Idle      |
//!
//! A fresh (non-repeat) down in any non-Idle state is a stale press that
//! never saw its release: it restarts from Down. A repeat down never changes
//! state, unless the state is older than `hold + 1 s`, when it counts as fresh.

use std::time::{Duration, Instant};

/// How the CapsLock key behaves.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CapsMode {
    #[default]
    Off,
    TapSwitches,
    CapsDisabled,
}

impl CapsMode {
    pub fn name(self) -> &'static str {
        match self {
            CapsMode::Off => "off",
            CapsMode::TapSwitches => "tap",
            CapsMode::CapsDisabled => "disabled",
        }
    }

    pub fn from_name(s: &str) -> Option<CapsMode> {
        match s {
            "off" => Some(CapsMode::Off),
            "tap" => Some(CapsMode::TapSwitches),
            "disabled" => Some(CapsMode::CapsDisabled),
            _ => None,
        }
    }
}

pub const HOLD_MIN_MS: u32 = 300;
pub const HOLD_MAX_MS: u32 = 1000;
pub const HOLD_DEFAULT_MS: u32 = 500;

pub fn clamp_hold_ms(ms: u32) -> u32 {
    ms.clamp(HOLD_MIN_MS, HOLD_MAX_MS)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    None,
    Switch,
    ToggleCapsLock,
    SwitchThenReplay,
}

#[derive(Clone, Copy, Debug)]
enum State {
    Idle,
    Down { since: Instant },
    Chorded { since: Instant },
    HoldFired { since: Instant },
}

pub struct CapsKey {
    hold: Duration,
    state: State,
}

impl CapsKey {
    pub fn new(hold_ms: u32) -> CapsKey {
        CapsKey {
            hold: Duration::from_millis(clamp_hold_ms(hold_ms) as u64),
            state: State::Idle,
        }
    }

    pub fn set_hold_ms(&mut self, ms: u32) {
        self.hold = Duration::from_millis(clamp_hold_ms(ms) as u64);
    }

    fn since(&self) -> Option<Instant> {
        match self.state {
            State::Idle => None,
            State::Down { since } | State::Chorded { since } | State::HoldFired { since } => {
                Some(since)
            }
        }
    }

    pub fn caps_down(&mut self, now: Instant, repeat: bool) -> Action {
        let stale = self
            .since()
            .is_some_and(|s| now.saturating_duration_since(s) > self.hold + Duration::from_secs(1));
        match (self.state, repeat) {
            (State::Idle, true) => {}
            (_, true) if !stale => {}
            _ => self.state = State::Down { since: now },
        }
        Action::None
    }

    pub fn caps_up(&mut self, now: Instant) -> Action {
        let prev = std::mem::replace(&mut self.state, State::Idle);
        match prev {
            State::Down { since } => {
                if now.saturating_duration_since(since) >= self.hold {
                    Action::ToggleCapsLock
                } else {
                    Action::Switch
                }
            }
            _ => Action::None,
        }
    }

    pub fn other_key_down(&mut self, _now: Instant) -> Action {
        match self.state {
            State::Down { since } => {
                self.state = State::Chorded { since };
                Action::SwitchThenReplay
            }
            _ => Action::None,
        }
    }

    pub fn hold_elapsed(&mut self, now: Instant) -> Action {
        match self.state {
            State::Down { since } if now.saturating_duration_since(since) >= self.hold => {
                self.state = State::HoldFired { since };
                Action::ToggleCapsLock
            }
            _ => Action::None,
        }
    }

    pub fn focus_changed(&mut self) {
        self.state = State::Idle;
    }

    pub fn is_pressed(&self) -> bool {
        !matches!(self.state, State::Idle)
    }

    pub fn hold(&self) -> Duration {
        self.hold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn tap_before_hold_switches() {
        let t0 = Instant::now();
        let mut k = CapsKey::new(500);
        assert_eq!(k.caps_down(t0, false), Action::None);
        assert!(k.is_pressed());
        assert_eq!(k.caps_up(t0 + ms(100)), Action::Switch);
        assert!(!k.is_pressed());
    }

    #[test]
    fn hold_timer_toggles_once_then_release_is_silent() {
        let t0 = Instant::now();
        let mut k = CapsKey::new(500);
        k.caps_down(t0, false);
        assert_eq!(k.hold_elapsed(t0 + ms(500)), Action::ToggleCapsLock);
        assert_eq!(k.caps_up(t0 + ms(700)), Action::None);
    }

    #[test]
    fn release_after_hold_without_timer_toggles_once() {
        let t0 = Instant::now();
        let mut k = CapsKey::new(500);
        k.caps_down(t0, false);
        assert_eq!(k.caps_up(t0 + ms(600)), Action::ToggleCapsLock);
    }

    #[test]
    fn late_timer_after_release_is_ignored() {
        let t0 = Instant::now();
        let mut k = CapsKey::new(500);
        k.caps_down(t0, false);
        assert_eq!(k.caps_up(t0 + ms(100)), Action::Switch);
        assert_eq!(k.hold_elapsed(t0 + ms(500)), Action::None);
    }

    #[test]
    fn other_key_while_down_switches_and_replays() {
        let t0 = Instant::now();
        let mut k = CapsKey::new(500);
        k.caps_down(t0, false);
        assert_eq!(k.other_key_down(t0 + ms(50)), Action::SwitchThenReplay);
        assert_eq!(k.other_key_down(t0 + ms(60)), Action::None);
        assert_eq!(k.hold_elapsed(t0 + ms(500)), Action::None);
        assert_eq!(k.caps_up(t0 + ms(100)), Action::None);
    }

    #[test]
    fn other_key_when_idle_is_none() {
        let mut k = CapsKey::new(500);
        assert_eq!(k.other_key_down(Instant::now()), Action::None);
    }

    #[test]
    fn repeat_down_is_swallowed_without_resetting() {
        let t0 = Instant::now();
        let mut k = CapsKey::new(500);
        k.caps_down(t0, false);
        assert_eq!(k.caps_down(t0 + ms(40), true), Action::None);
        assert_eq!(k.caps_up(t0 + ms(100)), Action::Switch);
    }

    #[test]
    fn fresh_down_after_stale_state_resets() {
        let t0 = Instant::now();
        let mut k = CapsKey::new(500);
        k.caps_down(t0, false);
        assert_eq!(k.caps_down(t0 + ms(5000), false), Action::None);
        assert_eq!(k.caps_up(t0 + ms(5100)), Action::Switch);
    }

    #[test]
    fn repeat_down_while_idle_is_none_and_stays_idle() {
        let mut k = CapsKey::new(500);
        assert_eq!(k.caps_down(Instant::now(), true), Action::None);
        assert!(!k.is_pressed());
    }

    #[test]
    fn focus_changed_returns_to_idle() {
        let t0 = Instant::now();
        let mut k = CapsKey::new(500);
        k.caps_down(t0, false);
        k.focus_changed();
        assert_eq!(k.caps_up(t0 + ms(10)), Action::None);
    }

    #[test]
    fn hold_is_clamped() {
        assert_eq!(CapsKey::new(5).hold(), ms(300));
        assert_eq!(CapsKey::new(99999).hold(), ms(1000));
        assert_eq!(clamp_hold_ms(500), 500);
    }

    #[test]
    fn mode_names_round_trip() {
        for m in [CapsMode::Off, CapsMode::TapSwitches, CapsMode::CapsDisabled] {
            assert_eq!(CapsMode::from_name(m.name()), Some(m));
        }
        assert_eq!(CapsMode::from_name("x"), None);
    }
}
