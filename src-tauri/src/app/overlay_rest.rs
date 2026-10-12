//! Whether the Drop rests in the camera notch while Utter is idle (§4.2a).

/// A rectangle in Core Graphics global coordinates (origin at the top left of the main display).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// What the watcher sees on the display under the mouse.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RestFacts {
    /// The "Drop in the notch" setting.
    pub enabled: bool,
    /// An app is full screen there (film, game, slides).
    pub fullscreen: bool,
    pub locked: bool,
    pub asleep: bool,
    /// The display under the mouse.
    pub display: u32,
}

pub fn rest_visible(f: &RestFacts) -> bool {
    f.enabled && !f.fullscreen && !f.locked && !f.asleep
}

/// The window fills the whole display (a maximised window stops below the menu bar).
pub fn covers_screen(window: Rect, screen: Rect) -> bool {
    const SLACK: f64 = 1.0;
    (window.x - screen.x).abs() <= SLACK
        && (window.y - screen.y).abs() <= SLACK
        && (window.x + window.w - (screen.x + screen.w)).abs() <= SLACK
        && (window.y + window.h - (screen.y + screen.h)).abs() <= SLACK
}

/// A normal-layer window of another app that covers the display.
pub fn window_is_fullscreen(layer: i64, pid: i64, own_pid: i64, bounds: Rect, screen: Rect) -> bool {
    layer == 0 && pid != own_pid && covers_screen(bounds, screen)
}

#[cfg(target_os = "macos")]
pub use mac::probe;

#[cfg(target_os = "macos")]
mod mac {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::display::CGDisplay;
    use core_graphics::event::CGEvent;
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::CGPoint;
    use core_graphics::window::{
        copy_window_info, kCGNullWindowID, kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly,
    };

    use super::{window_is_fullscreen, Rect, RestFacts};

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
        fn CGGetDisplaysWithPoint(point: CGPoint, max: u32, displays: *mut u32, count: *mut u32) -> i32;
    }

    fn rect(r: core_graphics::geometry::CGRect) -> Rect {
        Rect { x: r.origin.x, y: r.origin.y, w: r.size.width, h: r.size.height }
    }

    fn number(d: &CFDictionary<CFString, CFType>, key: &str) -> Option<f64> {
        d.find(CFString::new(key))?.downcast::<CFNumber>()?.to_f64()
    }

    /// The display under the mouse and its frame. A sleeping display is not "active", so the
    /// active list would be empty exactly when we need to see that the display sleeps.
    fn cursor_display() -> Option<(u32, Rect)> {
        let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()?;
        let p = CGEvent::new(src).ok()?.location();
        let (mut id, mut n) = (0u32, 0u32);
        // SAFETY: room for exactly one display id.
        let err = unsafe { CGGetDisplaysWithPoint(p, 1, &mut id, &mut n) };
        (err == 0 && n == 1).then(|| (id, rect(CGDisplay::new(id).bounds())))
    }

    fn fullscreen_on(screen: Rect) -> bool {
        let Some(list) = copy_window_info(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID)
        else {
            return false;
        };
        let own = std::process::id() as i64;
        list.iter().any(|item| {
            // SAFETY: every element of the window list is a CFDictionary.
            let d: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
            let (Some(layer), Some(pid)) = (number(&d, "kCGWindowLayer"), number(&d, "kCGWindowOwnerPID")) else {
                return false;
            };
            let Some(b) = d.find(CFString::new("kCGWindowBounds")).and_then(|b| b.downcast::<CFDictionary>()) else {
                return false;
            };
            // SAFETY: the bounds entry is a CFDictionary with CFString keys and CFNumber values.
            let b: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_get_rule(b.as_concrete_TypeRef()) };
            let (Some(x), Some(y), Some(w), Some(h)) = (number(&b, "X"), number(&b, "Y"), number(&b, "Width"), number(&b, "Height"))
            else {
                return false;
            };
            window_is_fullscreen(layer as i64, pid as i64, own, Rect { x, y, w, h }, screen)
        })
    }

    fn locked() -> bool {
        // SAFETY: a Copy-rule CF object or null.
        let raw = unsafe { CGSessionCopyCurrentDictionary() };
        if raw.is_null() {
            return false;
        }
        let d: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_create_rule(raw) };
        d.find(CFString::new("CGSSessionScreenIsLocked"))
            .and_then(|v| v.downcast::<CFBoolean>())
            .map(bool::from)
            .unwrap_or(false)
    }

    /// Reads the facts for the display under the mouse. Safe off the main thread.
    pub fn probe(enabled: bool) -> RestFacts {
        let Some((display, screen)) = cursor_display() else {
            return RestFacts { enabled, ..Default::default() };
        };
        RestFacts {
            enabled,
            fullscreen: fullscreen_on(screen),
            locked: locked(),
            asleep: CGDisplay::new(display).is_asleep(),
            display,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { x: 0.0, y: 0.0, w: 1512.0, h: 982.0 };

    #[test]
    fn rest_shown_only_when_nothing_blocks() {
        for bits in 0..16u8 {
            let f = RestFacts {
                enabled: bits & 1 != 0,
                fullscreen: bits & 2 != 0,
                locked: bits & 4 != 0,
                asleep: bits & 8 != 0,
                display: 1,
            };
            assert_eq!(rest_visible(&f), bits == 1, "{f:?}");
        }
    }

    #[test]
    fn fullscreen_window_covers_whole_screen() {
        assert!(covers_screen(SCREEN, SCREEN));
        assert!(covers_screen(Rect { x: 0.5, y: 0.0, w: 1511.5, h: 982.5 }, SCREEN));
    }

    #[test]
    fn maximized_window_is_not_fullscreen() {
        assert!(!covers_screen(Rect { x: 0.0, y: 37.0, w: 1512.0, h: 945.0 }, SCREEN));
    }

    #[test]
    fn fullscreen_on_other_display_does_not_count() {
        assert!(!covers_screen(Rect { x: 1512.0, y: 0.0, w: 1920.0, h: 1080.0 }, SCREEN));
    }

    #[test]
    fn window_is_fullscreen_rules() {
        assert!(!window_is_fullscreen(25, 42, 7, SCREEN, SCREEN), "not a normal window layer");
        assert!(!window_is_fullscreen(0, 7, 7, SCREEN, SCREEN), "our own window");
        assert!(window_is_fullscreen(0, 42, 7, SCREEN, SCREEN));
    }

    /// Reads the real system; run by hand: `cargo test --lib probe_runs -- --ignored --nocapture`.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn probe_runs() {
        for _ in 0..3 {
            let t = std::time::Instant::now();
            let f = probe(true);
            println!("{f:?} in {:?}", t.elapsed());
        }
    }

}
