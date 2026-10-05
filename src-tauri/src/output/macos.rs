//! Synthetic keyboard input on macOS (requires Accessibility permission).

use std::time::{Duration, Instant};

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

use super::InputPlatform;

/// Physical key positions: work the same in every keyboard layout.
const KEY_V: CGKeyCode = 9;
const KEY_RETURN: CGKeyCode = 36;
const KEY_A: CGKeyCode = 0;

const MODIFIER_MASK: u64 = 0x0002_0000 // shift
    | 0x0004_0000 // control
    | 0x0008_0000 // option
    | 0x0010_0000 // command
    | 0x0080_0000; // fn

extern "C" {
    fn CGEventSourceFlagsState(state_id: i32) -> u64;
}

pub struct MacInput;

impl MacInput {
    fn source() -> Result<CGEventSource, String> {
        CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| "cannot create event source".to_string())
    }

    fn key(code: CGKeyCode, flags: CGEventFlags) -> Result<(), String> {
        for down in [true, false] {
            let ev = CGEvent::new_keyboard_event(Self::source()?, code, down)
                .map_err(|_| "cannot create key event".to_string())?;
            // Explicit flags: a still-held Option must not turn ⌘V into ⌥⌘V.
            ev.set_flags(flags);
            ev.post(CGEventTapLocation::HID);
        }
        Ok(())
    }
}

impl InputPlatform for MacInput {
    fn frontmost_is_self(&self) -> bool {
        use objc2_app_kit::NSWorkspace;
        let workspace = NSWorkspace::sharedWorkspace();
        workspace
            .frontmostApplication()
            .map(|app| app.processIdentifier() as u32 == std::process::id())
            .unwrap_or(false)
    }

    fn frontmost_is_elevated(&self) -> bool {
        false
    }

    fn wait_modifiers_released(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        // 0 = kCGEventSourceStateCombinedSessionState
        while unsafe { CGEventSourceFlagsState(0) } & MODIFIER_MASK != 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn send_paste(&self) -> Result<(), String> {
        Self::key(KEY_V, CGEventFlags::CGEventFlagCommand)
    }

    fn type_text(&self, chunk: &str) -> Result<(), String> {
        for down in [true, false] {
            let ev = CGEvent::new_keyboard_event(Self::source()?, KEY_A, down)
                .map_err(|_| "cannot create key event".to_string())?;
            ev.set_flags(CGEventFlags::CGEventFlagNull);
            ev.set_string(chunk);
            ev.post(CGEventTapLocation::HID);
        }
        Ok(())
    }

    fn send_shift_enter(&self) -> Result<(), String> {
        Self::key(KEY_RETURN, CGEventFlags::CGEventFlagShift)
    }
}
