//! Recording state machine. Pure logic: no I/O, time is passed in.

use std::time::{Duration, Instant};

use crate::settings::RecordMode;
use crate::types::Mode;

pub const MIN_RECORDING: Duration = Duration::from_millis(300);
pub const MAX_RECORDING: Duration = Duration::from_secs(600);
pub const WARN_BEFORE: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyEvent {
    Down(Mode),
    Up(Mode),
    /// A key that reports presses but no releases (Caps Lock on macOS).
    Tap(Mode),
    /// Any non-hotkey key pressed while a hotkey is held.
    OtherKey,
    Escape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelReason {
    TooShort,
    Escape,
    Chord,
    Aborted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Start(Mode),
    Submit { mode: Mode, duration: Duration },
    Cancel(CancelReason),
    LimitWarning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Recording { mode: Mode, started: Instant, warned: bool },
}

pub struct Controller {
    record_mode: RecordMode,
    /// Record mode captured when the current recording started.
    active_mode: RecordMode,
    state: State,
}

impl Controller {
    pub fn new(mode: RecordMode) -> Self {
        Self { record_mode: mode, active_mode: mode, state: State::Idle }
    }

    /// Takes effect from the next recording.
    pub fn set_record_mode(&mut self, mode: RecordMode) {
        self.record_mode = mode;
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn handle(&mut self, ev: HotkeyEvent, now: Instant) -> Vec<Command> {
        let State::Recording { mode, started, .. } = self.state else {
            return match ev {
                HotkeyEvent::Down(m) | HotkeyEvent::Tap(m) => {
                    self.active_mode = self.record_mode;
                    self.state = State::Recording { mode: m, started: now, warned: false };
                    vec![Command::Start(m)]
                }
                _ => vec![],
            };
        };

        let hold = self.active_mode == RecordMode::Hold;
        match ev {
            HotkeyEvent::Escape => self.cancel(CancelReason::Escape),
            HotkeyEvent::Up(m) if m == mode && hold => self.finish(mode, started, now),
            HotkeyEvent::Down(m) if m == mode && !hold => self.finish(mode, started, now),
            HotkeyEvent::Tap(m) if m == mode => self.finish(mode, started, now),
            HotkeyEvent::Down(m) | HotkeyEvent::Tap(m) if m != mode && hold => {
                self.cancel(CancelReason::Chord)
            }
            HotkeyEvent::OtherKey if hold => self.cancel(CancelReason::Chord),
            _ => vec![],
        }
    }

    pub fn tick(&mut self, now: Instant) -> Vec<Command> {
        let State::Recording { mode, started, warned } = self.state else {
            return vec![];
        };
        let elapsed = now.saturating_duration_since(started);
        if elapsed >= MAX_RECORDING {
            return self.finish(mode, started, now);
        }
        if !warned && elapsed >= MAX_RECORDING - WARN_BEFORE {
            self.state = State::Recording { mode, started, warned: true };
            return vec![Command::LimitWarning];
        }
        vec![]
    }

    /// Ends a recording that could not continue (e.g. the microphone failed).
    pub fn abort(&mut self) -> Vec<Command> {
        match self.state {
            State::Recording { .. } => self.cancel(CancelReason::Aborted),
            State::Idle => vec![],
        }
    }

    fn finish(&mut self, mode: Mode, started: Instant, now: Instant) -> Vec<Command> {
        let duration = now.saturating_duration_since(started);
        if duration < MIN_RECORDING {
            return self.cancel(CancelReason::TooShort);
        }
        self.state = State::Idle;
        vec![Command::Submit { mode, duration }]
    }

    fn cancel(&mut self, reason: CancelReason) -> Vec<Command> {
        self.state = State::Idle;
        vec![Command::Cancel(reason)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Mode::{Dictation as D, Translate as T};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn hold_press_release_submits() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Hold);
        assert_eq!(c.handle(HotkeyEvent::Down(D), t0), vec![Command::Start(D)]);
        assert_eq!(
            c.handle(HotkeyEvent::Up(D), t0 + ms(1000)),
            vec![Command::Submit { mode: D, duration: ms(1000) }]
        );
        assert!(matches!(c.state(), State::Idle));
    }

    #[test]
    fn hold_short_press_cancels() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Hold);
        c.handle(HotkeyEvent::Down(D), t0);
        assert_eq!(
            c.handle(HotkeyEvent::Up(D), t0 + ms(200)),
            vec![Command::Cancel(CancelReason::TooShort)]
        );
        assert!(matches!(c.state(), State::Idle));
    }

    #[test]
    fn hold_exactly_min_duration_submits() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Hold);
        c.handle(HotkeyEvent::Down(D), t0);
        assert_eq!(
            c.handle(HotkeyEvent::Up(D), t0 + MIN_RECORDING),
            vec![Command::Submit { mode: D, duration: MIN_RECORDING }]
        );
    }

    #[test]
    fn hold_other_key_cancels_chord() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Hold);
        c.handle(HotkeyEvent::Down(D), t0);
        assert_eq!(
            c.handle(HotkeyEvent::OtherKey, t0 + ms(50)),
            vec![Command::Cancel(CancelReason::Chord)]
        );
        // The release that follows the chord must not start or submit anything.
        assert_eq!(c.handle(HotkeyEvent::Up(D), t0 + ms(80)), vec![]);
    }

    #[test]
    fn hold_second_hotkey_cancels_chord() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Hold);
        c.handle(HotkeyEvent::Down(D), t0);
        assert_eq!(
            c.handle(HotkeyEvent::Down(T), t0 + ms(500)),
            vec![Command::Cancel(CancelReason::Chord)]
        );
    }

    #[test]
    fn hold_repeated_down_is_ignored() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Hold);
        c.handle(HotkeyEvent::Down(D), t0);
        assert_eq!(c.handle(HotkeyEvent::Down(D), t0 + ms(500)), vec![]);
        assert!(matches!(c.state(), State::Recording { .. }));
    }

    #[test]
    fn toggle_two_presses_submit() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Toggle);
        assert_eq!(c.handle(HotkeyEvent::Down(D), t0), vec![Command::Start(D)]);
        assert_eq!(c.handle(HotkeyEvent::Up(D), t0 + ms(100)), vec![]);
        assert_eq!(
            c.handle(HotkeyEvent::Down(D), t0 + ms(2000)),
            vec![Command::Submit { mode: D, duration: ms(2000) }]
        );
    }

    #[test]
    fn toggle_quick_second_press_cancels() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Toggle);
        c.handle(HotkeyEvent::Down(D), t0);
        assert_eq!(
            c.handle(HotkeyEvent::Down(D), t0 + ms(150)),
            vec![Command::Cancel(CancelReason::TooShort)]
        );
    }

    #[test]
    fn toggle_ignores_up_and_other_keys() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Toggle);
        c.handle(HotkeyEvent::Down(D), t0);
        assert_eq!(c.handle(HotkeyEvent::Up(D), t0 + ms(400)), vec![]);
        assert_eq!(c.handle(HotkeyEvent::OtherKey, t0 + ms(500)), vec![]);
        assert!(matches!(c.state(), State::Recording { mode: D, .. }));
    }

    #[test]
    fn toggle_second_hotkey_ignored() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Toggle);
        c.handle(HotkeyEvent::Down(D), t0);
        assert_eq!(c.handle(HotkeyEvent::Down(T), t0 + ms(500)), vec![]);
        assert!(matches!(c.state(), State::Recording { mode: D, .. }));
    }

    #[test]
    fn escape_cancels_in_both_modes() {
        for mode in [RecordMode::Hold, RecordMode::Toggle] {
            let t0 = Instant::now();
            let mut c = Controller::new(mode);
            c.handle(HotkeyEvent::Down(T), t0);
            assert_eq!(
                c.handle(HotkeyEvent::Escape, t0 + ms(800)),
                vec![Command::Cancel(CancelReason::Escape)]
            );
            assert!(matches!(c.state(), State::Idle));
        }
    }

    #[test]
    fn tap_toggles_in_hold_mode() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Hold);
        assert_eq!(c.handle(HotkeyEvent::Tap(D), t0), vec![Command::Start(D)]);
        assert_eq!(
            c.handle(HotkeyEvent::Tap(D), t0 + ms(3000)),
            vec![Command::Submit { mode: D, duration: ms(3000) }]
        );
    }

    #[test]
    fn limit_warning_once_then_auto_submit() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Hold);
        c.handle(HotkeyEvent::Down(D), t0);
        assert_eq!(c.tick(t0 + Duration::from_secs(100)), vec![]);
        assert_eq!(c.tick(t0 + Duration::from_secs(570)), vec![Command::LimitWarning]);
        assert_eq!(c.tick(t0 + Duration::from_secs(580)), vec![]);
        assert_eq!(
            c.tick(t0 + Duration::from_secs(600)),
            vec![Command::Submit { mode: D, duration: Duration::from_secs(600) }]
        );
        assert!(matches!(c.state(), State::Idle));
    }

    #[test]
    fn abort_returns_to_idle() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Hold);
        c.handle(HotkeyEvent::Down(D), t0);
        assert_eq!(c.abort(), vec![Command::Cancel(CancelReason::Aborted)]);
        assert!(matches!(c.state(), State::Idle));
        assert_eq!(c.abort(), vec![]);
        assert_eq!(c.handle(HotkeyEvent::Down(D), t0 + ms(10)), vec![Command::Start(D)]);
    }

    #[test]
    fn idle_ignores_up_escape_other() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Hold);
        assert_eq!(c.handle(HotkeyEvent::Up(D), t0), vec![]);
        assert_eq!(c.handle(HotkeyEvent::Escape, t0), vec![]);
        assert_eq!(c.handle(HotkeyEvent::OtherKey, t0), vec![]);
        assert_eq!(c.tick(t0 + Duration::from_secs(700)), vec![]);
    }

    #[test]
    fn mode_change_applies_to_next_recording() {
        let t0 = Instant::now();
        let mut c = Controller::new(RecordMode::Hold);
        c.handle(HotkeyEvent::Down(D), t0);
        c.set_record_mode(RecordMode::Toggle);
        // Still behaves as Hold for the current recording.
        assert_eq!(
            c.handle(HotkeyEvent::Up(D), t0 + ms(1000)),
            vec![Command::Submit { mode: D, duration: ms(1000) }]
        );
        // Next recording is Toggle: release is ignored.
        c.handle(HotkeyEvent::Down(D), t0 + ms(2000));
        assert_eq!(c.handle(HotkeyEvent::Up(D), t0 + ms(3000)), vec![]);
    }
}
