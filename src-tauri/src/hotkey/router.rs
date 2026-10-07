//! Turns raw key transitions into hotkey events. Pure logic.

use std::collections::HashSet;

use tokio::sync::oneshot;

use crate::controller::HotkeyEvent;
use crate::settings::Hotkeys;
use crate::types::{KeyId, Mode};

/// What to do with one key transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Routed {
    pub event: Option<HotkeyEvent>,
    /// Drop the key so the focused app never sees it.
    pub swallow: bool,
}

pub struct Router {
    bindings: Hotkeys,
    held: HashSet<KeyId>,
    /// A recording is in progress: Escape and chord keys are consumed.
    recording: bool,
    /// Keys whose press was swallowed; their release is swallowed too.
    swallowed: HashSet<KeyId>,
    capture: Option<oneshot::Sender<KeyId>>,
    /// Key taken by capture whose release must be swallowed.
    captured: Option<KeyId>,
}

impl Router {
    pub fn new(bindings: Hotkeys) -> Self {
        Self {
            bindings,
            held: HashSet::new(),
            recording: false,
            swallowed: HashSet::new(),
            capture: None,
            captured: None,
        }
    }

    pub fn set_bindings(&mut self, bindings: Hotkeys) {
        self.bindings = bindings;
        self.held.clear();
    }

    /// The next key pressed (except Escape, which cancels) goes to `tx` instead of routing.
    pub fn begin_capture(&mut self, tx: oneshot::Sender<KeyId>) {
        self.capture = Some(tx);
    }

    fn mode_of(&self, key: &KeyId) -> Option<Mode> {
        if *key == self.bindings.dictation {
            Some(Mode::Dictation)
        } else if *key == self.bindings.translate {
            Some(Mode::Translate)
        } else {
            None
        }
    }

    /// Auto-repeat of a key whose press was swallowed must be swallowed too.
    pub fn swallows_repeat(&self, key: &KeyId) -> bool {
        self.swallowed.contains(key)
    }

    pub fn set_recording(&mut self, on: bool) {
        self.recording = on;
    }

    pub fn route(&mut self, key: KeyId, down: bool, tap_only: bool) -> Option<HotkeyEvent> {
        self.filter(key, down, tap_only).event
    }

    /// Routes a key transition and decides whether the focused app may see it.
    pub fn filter(&mut self, key: KeyId, down: bool, tap_only: bool) -> Routed {
        if !down && self.swallowed.remove(&key) {
            return Routed { event: None, swallow: true };
        }
        let event = self.route_event(key.clone(), down, tap_only);
        let consumable = matches!(event, Some(HotkeyEvent::Escape | HotkeyEvent::OtherKey));
        let swallow = down && self.recording && consumable;
        if swallow {
            self.swallowed.insert(key);
        }
        Routed { event, swallow }
    }

    fn route_event(&mut self, key: KeyId, down: bool, tap_only: bool) -> Option<HotkeyEvent> {
        if down {
            if let Some(tx) = self.capture.take() {
                if key != KeyId::Escape {
                    let _ = tx.send(key.clone());
                    self.captured = (!tap_only).then_some(key);
                }
                return None;
            }
        } else if self.captured.as_ref() == Some(&key) {
            self.captured = None;
            return None;
        }

        let mode = self.mode_of(&key);
        if tap_only {
            return mode.map(HotkeyEvent::Tap);
        }
        match (mode, down) {
            (Some(m), true) => self.held.insert(key).then_some(HotkeyEvent::Down(m)),
            (Some(m), false) => {
                self.held.remove(&key);
                Some(HotkeyEvent::Up(m))
            }
            (None, true) if key == KeyId::Escape => Some(HotkeyEvent::Escape),
            (None, true) if !self.held.is_empty() => Some(HotkeyEvent::OtherKey),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Mode::{Dictation as D, Translate as T};

    fn router() -> Router {
        Router::new(Hotkeys { dictation: KeyId::RightAlt, translate: KeyId::RightMeta })
    }

    #[test]
    fn router_down_up_for_bound_key() {
        let mut r = router();
        assert_eq!(r.route(KeyId::RightAlt, true, false), Some(HotkeyEvent::Down(D)));
        assert_eq!(r.route(KeyId::RightAlt, false, false), Some(HotkeyEvent::Up(D)));
        assert_eq!(r.route(KeyId::RightMeta, true, false), Some(HotkeyEvent::Down(T)));
    }

    #[test]
    fn router_ignores_autorepeat() {
        let mut r = router();
        assert_eq!(r.route(KeyId::RightAlt, true, false), Some(HotkeyEvent::Down(D)));
        assert_eq!(r.route(KeyId::RightAlt, true, false), None);
        assert_eq!(r.route(KeyId::RightAlt, true, false), None);
        assert_eq!(r.route(KeyId::RightAlt, false, false), Some(HotkeyEvent::Up(D)));
        assert_eq!(r.route(KeyId::RightAlt, true, false), Some(HotkeyEvent::Down(D)));
    }

    #[test]
    fn router_other_key_only_while_held() {
        let mut r = router();
        assert_eq!(r.route(KeyId::Raw(0), true, false), None);
        r.route(KeyId::RightAlt, true, false);
        assert_eq!(r.route(KeyId::Raw(0), true, false), Some(HotkeyEvent::OtherKey));
        assert_eq!(r.route(KeyId::Raw(0), false, false), None);
    }

    #[test]
    fn router_escape_always_reported() {
        let mut r = router();
        assert_eq!(r.route(KeyId::Escape, true, false), Some(HotkeyEvent::Escape));
        r.route(KeyId::RightAlt, true, false);
        assert_eq!(r.route(KeyId::Escape, true, false), Some(HotkeyEvent::Escape));
        assert_eq!(r.route(KeyId::Escape, false, false), None);
    }

    #[test]
    fn router_unbound_key_is_silent() {
        let mut r = router();
        assert_eq!(r.route(KeyId::F(13), true, false), None);
        assert_eq!(r.route(KeyId::F(13), false, false), None);
    }

    #[test]
    fn caps_lock_emits_tap() {
        let mut r = Router::new(Hotkeys { dictation: KeyId::CapsLock, translate: KeyId::RightMeta });
        assert_eq!(r.route(KeyId::CapsLock, true, true), Some(HotkeyEvent::Tap(D)));
        assert_eq!(r.route(KeyId::CapsLock, true, true), Some(HotkeyEvent::Tap(D)));
    }

    #[test]
    fn set_bindings_clears_held_keys() {
        let mut r = router();
        r.route(KeyId::RightAlt, true, false);
        r.set_bindings(Hotkeys { dictation: KeyId::F(13), translate: KeyId::F(14) });
        assert_eq!(r.route(KeyId::Raw(0), true, false), None);
        assert_eq!(r.route(KeyId::F(13), true, false), Some(HotkeyEvent::Down(D)));
    }

    #[test]
    fn capture_takes_next_key_and_is_not_routed() {
        let mut r = router();
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        r.begin_capture(tx);
        assert_eq!(r.route(KeyId::RightAlt, true, false), None);
        assert_eq!(rx.try_recv(), Ok(KeyId::RightAlt));
        // Its release is swallowed too, so no stray Up reaches the controller.
        assert_eq!(r.route(KeyId::RightAlt, false, false), None);
        assert_eq!(r.route(KeyId::RightAlt, true, false), Some(HotkeyEvent::Down(D)));
    }

    #[test]
    fn escape_cancels_capture() {
        let mut r = router();
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        r.begin_capture(tx);
        assert_eq!(r.route(KeyId::Escape, true, false), None);
        assert!(rx.try_recv().is_err());
        assert_eq!(r.route(KeyId::RightAlt, true, false), Some(HotkeyEvent::Down(D)));
    }

    #[test]
    fn swallows_other_key_while_recording_and_its_release() {
        let mut r = router();
        r.filter(KeyId::RightAlt, true, false);
        r.set_recording(true);
        assert_eq!(r.filter(KeyId::Raw(14), true, false), Routed { event: Some(HotkeyEvent::OtherKey), swallow: true });
        assert_eq!(r.filter(KeyId::Raw(14), false, false), Routed { event: None, swallow: true });
        // The next press of the same key after recording ends is delivered normally.
        r.set_recording(false);
        r.filter(KeyId::RightAlt, false, false);
        assert_eq!(r.filter(KeyId::Raw(14), true, false), Routed { event: None, swallow: false });
    }

    #[test]
    fn other_key_is_not_swallowed_when_not_recording() {
        let mut r = router();
        r.filter(KeyId::RightAlt, true, false);
        assert_eq!(r.filter(KeyId::Raw(14), true, false), Routed { event: Some(HotkeyEvent::OtherKey), swallow: false });
    }

    #[test]
    fn swallows_escape_while_recording() {
        let mut r = router();
        r.set_recording(true);
        assert_eq!(r.filter(KeyId::Escape, true, false), Routed { event: Some(HotkeyEvent::Escape), swallow: true });
        assert_eq!(r.filter(KeyId::Escape, false, false), Routed { event: None, swallow: true });
    }

    #[test]
    fn escape_passes_through_when_idle() {
        let mut r = router();
        assert_eq!(r.filter(KeyId::Escape, true, false), Routed { event: Some(HotkeyEvent::Escape), swallow: false });
    }

    #[test]
    fn hotkeys_themselves_are_never_swallowed() {
        let mut r = router();
        r.set_recording(true);
        assert!(!r.filter(KeyId::RightAlt, true, false).swallow);
        assert!(!r.filter(KeyId::RightAlt, false, false).swallow);
    }

    #[test]
    fn repeats_of_swallowed_keys_are_swallowed() {
        let mut r = router();
        r.set_recording(true);
        r.filter(KeyId::Escape, true, false);
        assert!(r.swallows_repeat(&KeyId::Escape));
        assert!(!r.swallows_repeat(&KeyId::Raw(14)));
        r.filter(KeyId::Escape, false, false);
        assert!(!r.swallows_repeat(&KeyId::Escape));
    }
}
