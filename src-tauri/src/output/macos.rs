//! Synthetic keyboard input on macOS (requires Accessibility permission).

use std::time::{Duration, Instant};

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode, EventField};

use crate::hotkey::macos::SYNTHETIC_MARK;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

use super::{CaretContext, InputPlatform};

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

mod ax {
    use std::ffi::c_void;

    use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};

    use super::CaretContext;

    type AXUIElementRef = *const c_void;
    const AX_VALUE_CFRANGE: u32 = 4;
    /// Never block delivery on an unresponsive app.
    const AX_TIMEOUT_SECS: f32 = 0.25;

    #[repr(C)]
    struct CFRange {
        location: isize,
        length: isize,
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout: f32) -> i32;
        fn AXUIElementCopyAttributeValue(element: AXUIElementRef, attr: CFStringRef, value: *mut CFTypeRef) -> i32;
        fn AXUIElementCopyParameterizedAttributeValue(
            element: AXUIElementRef,
            attr: CFStringRef,
            param: CFTypeRef,
            result: *mut CFTypeRef,
        ) -> i32;
        fn AXValueCreate(the_type: u32, value_ptr: *const c_void) -> CFTypeRef;
        fn AXValueGetValue(value: CFTypeRef, the_type: u32, value_ptr: *mut c_void) -> bool;
    }

    unsafe fn copy_attr(element: AXUIElementRef, name: &str) -> Option<CFTypeRef> {
        let attr = CFString::new(name);
        let mut out: CFTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut out);
        (err == 0 && !out.is_null()).then_some(out)
    }

    /// The character before the caret, read through the Accessibility API.
    pub fn caret_context() -> CaretContext {
        unsafe {
            let system = AXUIElementCreateSystemWide();
            if system.is_null() {
                return CaretContext::Unknown;
            }
            AXUIElementSetMessagingTimeout(system, AX_TIMEOUT_SECS);
            let focused = copy_attr(system, "AXFocusedUIElement");
            CFRelease(system as CFTypeRef);
            let Some(focused) = focused else { return CaretContext::Unknown };
            AXUIElementSetMessagingTimeout(focused as AXUIElementRef, AX_TIMEOUT_SECS);
            let result = char_before_caret(focused as AXUIElementRef);
            CFRelease(focused);
            result
        }
    }

    unsafe fn char_before_caret(field: AXUIElementRef) -> CaretContext {
        let Some(range_value) = copy_attr(field, "AXSelectedTextRange") else {
            return CaretContext::Unknown;
        };
        let mut range = CFRange { location: 0, length: 0 };
        let ok = AXValueGetValue(range_value, AX_VALUE_CFRANGE, &mut range as *mut CFRange as *mut c_void);
        CFRelease(range_value);
        if !ok || range.location < 0 {
            return CaretContext::Unknown;
        }
        if range.location == 0 {
            return CaretContext::StartOfField;
        }
        let prev = CFRange { location: range.location - 1, length: 1 };
        let param = AXValueCreate(AX_VALUE_CFRANGE, &prev as *const CFRange as *const c_void);
        if param.is_null() {
            return CaretContext::Unknown;
        }
        let attr = CFString::new("AXStringForRange");
        let mut out: CFTypeRef = std::ptr::null();
        let err = AXUIElementCopyParameterizedAttributeValue(field, attr.as_concrete_TypeRef(), param, &mut out);
        CFRelease(param);
        if err != 0 || out.is_null() {
            return CaretContext::Unknown;
        }
        let s = CFString::wrap_under_create_rule(out as CFStringRef).to_string();
        match s.chars().last() {
            Some(c) => CaretContext::Char(c),
            None => CaretContext::Unknown,
        }
    }
}

pub struct MacInput;

impl MacInput {
    fn source() -> Result<CGEventSource, String> {
        CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| "cannot create event source".to_string())
    }

    /// Presses and releases one key, marked as Utter's own (the hotkey tap ignores it).
    pub fn tap_key(code: CGKeyCode) -> Result<(), String> {
        Self::key(code, CGEventFlags::CGEventFlagNull)
    }

    fn key(code: CGKeyCode, flags: CGEventFlags) -> Result<(), String> {
        for down in [true, false] {
            let ev = CGEvent::new_keyboard_event(Self::source()?, code, down)
                .map_err(|_| "cannot create key event".to_string())?;
            // Explicit flags: a still-held Option must not turn ⌘V into ⌥⌘V.
            ev.set_flags(flags);
            ev.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, SYNTHETIC_MARK);
            ev.post(CGEventTapLocation::HID);
        }
        Ok(())
    }
}

impl InputPlatform for MacInput {
    fn caret_context(&self) -> CaretContext {
        ax::caret_context()
    }

    fn frontmost_pid(&self) -> Option<i32> {
        use objc2_app_kit::NSWorkspace;
        NSWorkspace::sharedWorkspace().frontmostApplication().map(|app| app.processIdentifier())
    }

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
            ev.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, SYNTHETIC_MARK);
            ev.post(CGEventTapLocation::HID);
        }
        Ok(())
    }

    fn send_shift_enter(&self) -> Result<(), String> {
        Self::key(KEY_RETURN, CGEventFlags::CGEventFlagShift)
    }
}
