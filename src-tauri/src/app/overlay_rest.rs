//! Whether the Drop rests in the camera notch while Utter is idle (§4.2a).

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

use super::orchestrator::Shared;
use super::overlay::{self, HideGate};

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

fn intersects(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

/// `windows` (layer, owner pid, bounds) front to back, as the window server lists them. Only
/// the frontmost normal window of another app on this display decides: a screen-sized window
/// lying behind others (an auto-hidden menu bar lets zoomed windows reach the top) is not a
/// full-screen app.
pub fn fullscreen_in(windows: &[(i64, i64, Rect)], own_pid: i64, screen: Rect) -> bool {
    windows
        .iter()
        .find(|&&(layer, pid, b)| layer == 0 && pid != own_pid && intersects(b, screen))
        .is_some_and(|&(layer, pid, b)| window_is_fullscreen(layer, pid, own_pid, b, screen))
}

/// How the overlay window stands, shared by the pill and the watcher.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Presence {
    /// A recording or its result is on screen (anything but hidden / cancelled).
    pub busy: bool,
    /// The ears are shown (or will be once the pill folds back).
    pub shown: bool,
    /// The display the ears were last placed on.
    pub display: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RestAction {
    None,
    /// Show the ears; `replace` moves the window to the display under the mouse first.
    Show { replace: bool },
    Hide,
}

/// What the watcher does with the latest facts. A busy pill is never moved.
pub fn decide(p: &Presence, f: &RestFacts) -> RestAction {
    if !rest_visible(f) {
        return if p.shown { RestAction::Hide } else { RestAction::None };
    }
    let moved = !p.busy && p.display != Some(f.display);
    if !p.shown || moved {
        RestAction::Show { replace: moved }
    } else {
        RestAction::None
    }
}

/// After the pill folds (hidden / cancelled), the window goes away only if no ears are shown.
pub fn hide_after_state(p: &Presence) -> bool {
    !p.shown
}

/// A new pill state. Returns whether to move the window to the display under the mouse: only
/// for the first state after idle — a recording, its result and the fold never change display
/// (§4.2a). Hidden / cancelled keep the pill busy until [`after_fold`].
pub fn on_pill_state(p: &mut Presence, hidden: bool) -> bool {
    if hidden {
        return false;
    }
    let fresh = !p.busy;
    p.busy = true;
    fresh
}

/// The island has folded with no newer state. Returns whether to order the window out.
pub fn after_fold(p: &mut Presence) -> bool {
    p.busy = false;
    hide_after_state(p)
}

#[cfg(target_os = "macos")]
pub use mac::probe;

#[cfg(not(target_os = "macos"))]
pub fn probe(enabled: bool) -> RestFacts {
    // Windows has no notch rest yet (Plan 3).
    let _ = enabled;
    RestFacts::default()
}

/// How often the watcher looks at the screen.
const PERIOD: std::time::Duration = std::time::Duration::from_secs(2);

/// Wakes the watcher at once (settings changed, the overlay page loaded).
pub struct RestWake(pub Arc<Notify>);

/// The shared presence, for the overlay page's "I have loaded" command.
pub struct RestPresence(pub Arc<Mutex<Presence>>);

/// The overlay page has (re)loaded and missed earlier events: show the ears again from scratch.
pub fn page_ready(presence: &Mutex<Presence>, wake: &Notify) {
    if let Ok(mut p) = presence.lock() {
        p.shown = false;
        p.display = None;
    }
    wake.notify_one();
}

/// Every [`PERIOD`] (or when woken) decides whether the ears are shown and moves them to the
/// display under the mouse while idle.
pub fn spawn_watcher(
    app: AppHandle,
    shared: Arc<Shared>,
    presence: Arc<Mutex<Presence>>,
    gate: Arc<Mutex<HideGate>>,
    wake: Arc<Notify>,
) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(PERIOD);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut last: Option<RestFacts> = None;
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                _ = wake.notified() => {}
            }
            let facts = probe(shared.settings().notch_drop);
            if last != Some(facts) {
                tracing::info!(?facts, "notch rest facts");
                last = Some(facts);
            }
            // Decide and record under the lock; act on the window without holding it.
            let (action, busy) = {
                let Ok(mut p) = presence.lock() else { continue };
                let action = decide(&p, &facts);
                match action {
                    RestAction::Show { replace } => {
                        p.shown = true;
                        if replace {
                            p.display = Some(facts.display);
                        }
                    }
                    RestAction::Hide => p.shown = false,
                    RestAction::None => {}
                }
                (action, p.busy)
            };
            match action {
                RestAction::None => {}
                RestAction::Show { replace } => {
                    let a = app.clone();
                    let _ = app.run_on_main_thread(move || overlay::show_rest(&a, replace));
                }
                RestAction::Hide => {
                    let _ = app.emit_to(overlay::LABEL, "overlay://rest", serde_json::json!({ "shown": false }));
                    if !busy {
                        let gen = match gate.lock() {
                            Ok(mut g) => g.on_state(),
                            Err(_) => continue,
                        };
                        let (app, gate, presence) = (app.clone(), gate.clone(), presence.clone());
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(overlay::HIDE_AFTER).await;
                            let a = app.clone();
                            // Checked on the main thread, where `show_rest` also runs: a show
                            // queued after this check would otherwise be undone by the order-out.
                            let _ = app.run_on_main_thread(move || {
                                let current = gate.lock().map(|g| g.still_current(gen)).unwrap_or(false);
                                let gone = presence.lock().map(|p| !p.busy && hide_after_state(&p)).unwrap_or(false);
                                if current && gone {
                                    overlay::hide_window(&a);
                                }
                            });
                        });
                    }
                }
            }
        }
    });
}

/// The mouse pointer in global top-left points.
#[cfg(target_os = "macos")]
pub fn cursor_point() -> Option<(f64, f64)> {
    mac::cursor_point()
}

#[cfg(not(target_os = "macos"))]
pub fn cursor_point() -> Option<(f64, f64)> {
    None
}

/// The frame (top-left points, global) of the display under the mouse.
#[cfg(target_os = "macos")]
pub fn cursor_screen() -> Option<Rect> {
    mac::cursor_display().map(|(_, r)| r)
}

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

    use super::{fullscreen_in, Rect, RestFacts};

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
    pub(super) fn cursor_point() -> Option<(f64, f64)> {
        let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()?;
        let p = CGEvent::new(src).ok()?.location();
        Some((p.x, p.y))
    }

    pub(super) fn cursor_display() -> Option<(u32, Rect)> {
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
        let windows: Vec<(i64, i64, Rect)> = list
            .iter()
            .filter_map(|item| {
                // SAFETY: every element of the window list is a CFDictionary.
                let d: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
                let layer = number(&d, "kCGWindowLayer")?;
                let pid = number(&d, "kCGWindowOwnerPID")?;
                let b = d.find(CFString::new("kCGWindowBounds"))?.downcast::<CFDictionary>()?;
                // SAFETY: the bounds entry is a CFDictionary with CFString keys and CFNumber values.
                let b: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_get_rule(b.as_concrete_TypeRef()) };
                let r = Rect { x: number(&b, "X")?, y: number(&b, "Y")?, w: number(&b, "Width")?, h: number(&b, "Height")? };
                Some((layer as i64, pid as i64, r))
            })
            .collect();
        fullscreen_in(&windows, std::process::id() as i64, screen)
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

    fn shown_facts(display: u32) -> RestFacts {
        RestFacts { enabled: true, display, ..Default::default() }
    }

    #[test]
    fn first_tick_shows_rest() {
        assert_eq!(decide(&Presence::default(), &shown_facts(1)), RestAction::Show { replace: true });
    }

    #[test]
    fn rest_hides_on_fullscreen() {
        let p = Presence { shown: true, display: Some(1), ..Default::default() };
        let f = RestFacts { fullscreen: true, ..shown_facts(1) };
        assert_eq!(decide(&p, &f), RestAction::Hide);
    }

    #[test]
    fn nothing_to_do_when_settled() {
        let p = Presence { shown: true, display: Some(1), ..Default::default() };
        assert_eq!(decide(&p, &shown_facts(1)), RestAction::None);
        let hidden = Presence::default();
        assert_eq!(decide(&hidden, &RestFacts { enabled: false, ..shown_facts(1) }), RestAction::None);
    }

    #[test]
    fn no_replace_while_busy() {
        let p = Presence { busy: true, shown: true, display: Some(1) };
        assert_eq!(decide(&p, &shown_facts(2)), RestAction::None);
    }

    #[test]
    fn shows_without_moving_while_busy() {
        let p = Presence { busy: true, shown: false, display: Some(1) };
        assert_eq!(decide(&p, &shown_facts(2)), RestAction::Show { replace: false });
    }

    #[test]
    fn moves_to_cursor_display_when_idle() {
        let p = Presence { busy: false, shown: true, display: Some(1) };
        assert_eq!(decide(&p, &shown_facts(2)), RestAction::Show { replace: true });
    }

    #[test]
    fn delayed_hide_skipped_when_rest_shown() {
        assert!(!hide_after_state(&Presence { shown: true, ..Default::default() }));
        assert!(hide_after_state(&Presence { shown: false, ..Default::default() }));
    }

    #[test]
    fn hide_target_follows_current_presence() {
        // The switch was turned off during a recording: the watcher hid the rest...
        let mut p = Presence { busy: true, shown: true, display: Some(1) };
        assert_eq!(decide(&p, &RestFacts { enabled: false, ..shown_facts(1) }), RestAction::Hide);
        p.shown = false;
        // ...so when the recording ends the window goes away instead of staying as ears.
        p.busy = false;
        assert!(hide_after_state(&p));
    }

    #[test]
    fn only_the_first_pill_state_after_idle_moves_the_window() {
        let mut p = Presence { display: Some(1), ..Default::default() };
        assert!(on_pill_state(&mut p, false), "recording starts: follow the mouse");
        assert!(!on_pill_state(&mut p, false), "limit warning");
        assert!(!on_pill_state(&mut p, false), "processing");
        assert!(!on_pill_state(&mut p, false), "done");
    }

    #[test]
    fn folding_island_is_still_busy() {
        let mut p = Presence { shown: true, display: Some(1), ..Default::default() };
        on_pill_state(&mut p, false);
        on_pill_state(&mut p, true); // hidden: the island folds for HIDE_AFTER
        assert!(p.busy);
        assert_eq!(decide(&p, &shown_facts(2)), RestAction::None, "no jump to the other display mid-fold");
        assert!(!on_pill_state(&mut p, false), "a recording during the fold stays where it is");
    }

    #[test]
    fn after_the_fold_the_window_goes_only_without_ears() {
        let mut p = Presence { busy: true, shown: true, display: Some(1) };
        assert!(!after_fold(&mut p));
        assert!(!p.busy);
        let mut p = Presence { busy: true, shown: false, display: Some(1) };
        assert!(after_fold(&mut p));
        let off = RestFacts { enabled: false, ..shown_facts(2) };
        assert_eq!(decide(&p, &off), RestAction::None, "ears off: nothing to show");
    }

    #[test]
    fn page_ready_forces_a_fresh_show() {
        let presence = Mutex::new(Presence { busy: false, shown: true, display: Some(1) });
        page_ready(&presence, &Notify::new());
        let p = *presence.lock().unwrap();
        assert_eq!(decide(&p, &shown_facts(1)), RestAction::Show { replace: true });
    }

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

    const OTHER: Rect = Rect { x: 1512.0, y: 0.0, w: 1920.0, h: 1080.0 };
    const SMALL: Rect = Rect { x: 100.0, y: 100.0, w: 600.0, h: 400.0 };

    #[test]
    fn frontmost_window_covering_the_screen_is_fullscreen() {
        assert!(fullscreen_in(&[(0, 42, SCREEN), (0, 43, SMALL)], 7, SCREEN));
    }

    #[test]
    fn a_screen_sized_window_behind_another_does_not_count() {
        assert!(!fullscreen_in(&[(0, 43, SMALL), (0, 42, SCREEN)], 7, SCREEN));
    }

    #[test]
    fn our_own_and_other_layers_and_other_displays_are_skipped() {
        let list = [(25, 50, SMALL), (0, 7, SMALL), (0, 44, OTHER), (0, 42, SCREEN)];
        assert!(fullscreen_in(&list, 7, SCREEN));
        assert!(!fullscreen_in(&[], 7, SCREEN));
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
