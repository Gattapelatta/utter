//! Global keyboard listening on macOS via a listen-only CGEventTap.

use crate::types::KeyId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub key: KeyId,
    pub down: bool,
    /// The key reports presses only, never releases (Caps Lock).
    pub tap_only: bool,
}

/// Stamped into `EVENT_SOURCE_USER_DATA` of every key event Utter posts itself,
/// so the listener never mistakes its own ⌘V for the user's keystroke.
pub const SYNTHETIC_MARK: i64 = 0x0055_5454_4552; // "UTTER"

pub fn is_synthetic(user_data: i64) -> bool {
    user_data == SYNTHETIC_MARK
}

/// Drops a key event. With this crate, returning `None` from the tap callback
/// passes the original event through, so the event is turned into a Null event
/// (which the system ignores) and returned instead.
pub fn swallow(event: &core_graphics::event::CGEvent) -> Option<core_graphics::event::CGEvent> {
    event.set_type(core_graphics::event::CGEventType::Null);
    Some(event.clone())
}

// Device-dependent modifier bits (NX_DEVICE*KEYMASK) distinguish left from right.
const NX_LCTL: u64 = 0x0001;
const NX_LSHIFT: u64 = 0x0002;
const NX_RSHIFT: u64 = 0x0004;
const NX_LCMD: u64 = 0x0008;
const NX_RCMD: u64 = 0x0010;
const NX_LALT: u64 = 0x0020;
const NX_RALT: u64 = 0x0040;
const NX_RCTL: u64 = 0x2000;
const FLAG_FN: u64 = 0x0080_0000;

/// Decodes a `flagsChanged` event into a press or release of one modifier key.
pub fn decode_flags_changed(keycode: u16, flags: u64) -> Option<Decoded> {
    let (key, mask) = match keycode {
        54 => (KeyId::RightMeta, NX_RCMD),
        55 => (KeyId::Raw(55), NX_LCMD),
        56 => (KeyId::Raw(56), NX_LSHIFT),
        58 => (KeyId::Raw(58), NX_LALT),
        59 => (KeyId::Raw(59), NX_LCTL),
        60 => (KeyId::RightShift, NX_RSHIFT),
        61 => (KeyId::RightAlt, NX_RALT),
        62 => (KeyId::RightCtrl, NX_RCTL),
        63 => (KeyId::Fn, FLAG_FN),
        57 => return Some(Decoded { key: KeyId::CapsLock, down: true, tap_only: true }),
        _ => return None,
    };
    Some(Decoded { key, down: flags & mask != 0, tap_only: false })
}

pub fn keycode_to_key(keycode: u16) -> KeyId {
    match keycode {
        53 => KeyId::Escape,
        122 => KeyId::F(1),
        120 => KeyId::F(2),
        99 => KeyId::F(3),
        118 => KeyId::F(4),
        96 => KeyId::F(5),
        97 => KeyId::F(6),
        98 => KeyId::F(7),
        100 => KeyId::F(8),
        101 => KeyId::F(9),
        109 => KeyId::F(10),
        103 => KeyId::F(11),
        111 => KeyId::F(12),
        105 => KeyId::F(13),
        107 => KeyId::F(14),
        113 => KeyId::F(15),
        106 => KeyId::F(16),
        64 => KeyId::F(17),
        79 => KeyId::F(18),
        80 => KeyId::F(19),
        90 => KeyId::F(20),
        other => KeyId::Raw(other as u32),
    }
}

use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use core_foundation::base::TCFType;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType, EventField,
};
use tokio::sync::mpsc;

use super::router::Router;
use super::HotkeyError;
use crate::controller::HotkeyEvent;

extern "C" {
    fn CGEventTapEnable(tap: *const std::ffi::c_void, enable: bool);
}

pub(super) fn stop_run_loop(rl: usize) {
    if rl != 0 {
        let rl = rl as core_foundation::runloop::CFRunLoopRef;
        unsafe { core_foundation::runloop::CFRunLoopStop(rl) };
    }
}

/// Runs the tap on its own thread with its own CFRunLoop.
pub(super) fn spawn_tap(
    router: Arc<Mutex<Router>>,
    tx: mpsc::UnboundedSender<HotkeyEvent>,
    run_loop: Arc<AtomicUsize>,
) -> Result<(), HotkeyError> {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), HotkeyError>>();
    std::thread::Builder::new()
        .name("utter-hotkeys".into())
        .spawn(move || {
            let port: Arc<AtomicPtr<std::ffi::c_void>> = Arc::new(AtomicPtr::new(std::ptr::null_mut()));
            let port_cb = port.clone();
            let tap = CGEventTap::new(
                CGEventTapLocation::Session,
                CGEventTapPlacement::HeadInsertEventTap,
                // Active (not listen-only): keys pressed during a recording can be swallowed.
                CGEventTapOptions::Default,
                vec![CGEventType::KeyDown, CGEventType::KeyUp, CGEventType::FlagsChanged],
                move |_proxy, etype, event| {
                    if is_synthetic(event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA)) {
                        return Some(event.clone());
                    }
                    let decoded = match etype {
                        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                            let p = port_cb.load(Ordering::SeqCst);
                            if !p.is_null() {
                                unsafe { CGEventTapEnable(p, true) };
                            }
                            tracing::warn!("event tap was disabled by the system; re-enabled");
                            None
                        }
                        CGEventType::FlagsChanged => {
                            let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                            decode_flags_changed(code, event.get_flags().bits())
                        }
                        CGEventType::KeyDown | CGEventType::KeyUp => {
                            let repeat = event.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT) != 0;
                            let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                            let down = matches!(etype, CGEventType::KeyDown);
                            let key = keycode_to_key(code);
                            if down && repeat {
                                // Auto-repeat never reaches the router, but repeats of a
                                // swallowed key (a held Esc) must not leak to the app.
                                let swallowed = router.lock().map(|r| r.swallows_repeat(&key)).unwrap_or(false);
                                return if swallowed { swallow(event) } else { Some(event.clone()) };
                            }
                            Some(Decoded { key, down, tap_only: false })
                        }
                        _ => None,
                    };
                    let Some(d) = decoded else { return Some(event.clone()) };
                    let routed = router.lock().ok().map(|mut r| r.filter(d.key, d.down, d.tap_only, std::time::Instant::now()));
                    let Some(routed) = routed else { return Some(event.clone()) };
                    if let Some(ev) = routed.event {
                        let _ = tx.send(ev);
                    }
                    if routed.swallow {
                        swallow(event)
                    } else {
                        Some(event.clone())
                    }
                },
            );
            let tap = match tap {
                Ok(t) => t,
                Err(()) => {
                    let _ = ready_tx.send(Err(HotkeyError::NoPermission));
                    return;
                }
            };
            port.store(tap.mach_port.as_concrete_TypeRef() as *mut _, Ordering::SeqCst);
            let source = match tap.mach_port.create_runloop_source(0) {
                Ok(s) => s,
                Err(()) => {
                    let _ = ready_tx.send(Err(HotkeyError::Tap("run loop source".into())));
                    return;
                }
            };
            let current = CFRunLoop::get_current();
            current.add_source(&source, unsafe { kCFRunLoopCommonModes });
            tap.enable();
            run_loop.store(current.as_concrete_TypeRef() as usize, Ordering::SeqCst);
            let _ = ready_tx.send(Ok(()));
            CFRunLoop::run_current();
            // Stopping the run loop alone leaves an enabled active tap that nobody
            // services: every key press then waits on it, and after many sleep/wake
            // restarts the keyboard stalls system-wide. Disable and remove it.
            unsafe { CGEventTapEnable(tap.mach_port.as_concrete_TypeRef() as *const _, false) };
            current.remove_source(&source, unsafe { kCFRunLoopCommonModes });
            unsafe { core_foundation::mach_port::CFMachPortInvalidate(tap.mach_port.as_concrete_TypeRef()) };
            tracing::info!("hotkey listener stopped");
        })
        .map_err(|e| HotkeyError::Tap(e.to_string()))?;
    ready_rx
        .recv()
        .unwrap_or_else(|_| Err(HotkeyError::Tap("listener thread exited".into())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_right_option_down_up() {
        assert_eq!(decode_flags_changed(61, 0x0008_0040), Some(Decoded { key: KeyId::RightAlt, down: true, tap_only: false }));
        assert_eq!(decode_flags_changed(61, 0x0000_0100), Some(Decoded { key: KeyId::RightAlt, down: false, tap_only: false }));
    }

    #[test]
    fn decode_right_option_up_while_left_still_held() {
        // Left Option (0x20) still down, right (0x40) released.
        assert_eq!(decode_flags_changed(61, 0x0008_0020).map(|d| d.down), Some(false));
    }

    #[test]
    fn decode_left_option_is_not_right() {
        assert_eq!(decode_flags_changed(58, 0x0008_0020), Some(Decoded { key: KeyId::Raw(58), down: true, tap_only: false }));
    }

    #[test]
    fn decode_right_command_ctrl_shift_fn() {
        assert_eq!(decode_flags_changed(54, 0x10).map(|d| d.key), Some(KeyId::RightMeta));
        assert_eq!(decode_flags_changed(62, 0x2000).map(|d| (d.key, d.down)), Some((KeyId::RightCtrl, true)));
        assert_eq!(decode_flags_changed(60, 0x04).map(|d| d.key), Some(KeyId::RightShift));
        assert_eq!(decode_flags_changed(63, 0x80_0000).map(|d| (d.key, d.down)), Some((KeyId::Fn, true)));
    }

    #[test]
    fn decode_caps_lock_is_tap_only() {
        assert_eq!(decode_flags_changed(57, 0x1_0000), Some(Decoded { key: KeyId::CapsLock, down: true, tap_only: true }));
        assert_eq!(decode_flags_changed(57, 0), Some(Decoded { key: KeyId::CapsLock, down: true, tap_only: true }));
    }

    #[test]
    fn decode_unknown_keycode_is_none() {
        assert_eq!(decode_flags_changed(12, 0), None);
    }

    #[test]
    fn keycode_table() {
        assert_eq!(keycode_to_key(105), KeyId::F(13));
        assert_eq!(keycode_to_key(80), KeyId::F(19));
        assert_eq!(keycode_to_key(122), KeyId::F(1));
        assert_eq!(keycode_to_key(53), KeyId::Escape);
        assert_eq!(keycode_to_key(0), KeyId::Raw(0));
    }

    #[test]
    fn synthetic_mark_is_recognised() {
        assert!(is_synthetic(SYNTHETIC_MARK));
        assert!(!is_synthetic(0));
    }

    #[test]
    fn swallowed_event_becomes_null() {
        use core_graphics::event::{CGEvent, CGEventType};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        let src = CGEventSource::new(CGEventSourceStateID::Private).unwrap();
        let ev = CGEvent::new_keyboard_event(src, 14, true).unwrap();
        let out = swallow(&ev).expect("an event must be returned to the system");
        assert_eq!(out.get_type() as u32, CGEventType::Null as u32);
    }
}
