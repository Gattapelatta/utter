//! The recording pill: a transparent, click-through, non-activating panel.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

use super::orchestrator::{OverlaySink, OverlayState};
use super::overlay_rest::{after_fold, on_pill_state, Presence};

pub const LABEL: &str = "overlay";
/// The window is ordered out only after the island has folded back into the notch
/// (600 / 350 ms tween in `islandMotion.ts`, plus a margin for a late frame).
pub const HIDE_AFTER: Duration = Duration::from_millis(800);
const CANCEL_HIDE_AFTER: Duration = Duration::from_millis(500);

/// The camera cut-out of a MacBook display, in points.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct Notch {
    pub width: f64,
    pub height: f64,
}

/// The island hangs from the top edge of the screen, centred (§4.2).
pub fn island_origin(screen_x: f64, screen_y: f64, screen_w: f64, window_w: f64) -> (f64, f64) {
    (screen_x + (screen_w - window_w) / 2.0, screen_y)
}

/// Each overlay state gets a generation; a delayed hide only runs if no newer state came.
#[derive(Default)]
pub struct HideGate {
    gen: u64,
}

impl HideGate {
    pub fn on_state(&mut self) -> u64 {
        self.gen += 1;
        self.gen
    }

    pub fn still_current(&self, gen: u64) -> bool {
        self.gen == gen
    }
}

/// When the last recording pill was requested, for the `overlay_painted` latency log.
#[derive(Default)]
pub struct OverlayTiming(pub Mutex<Option<Instant>>);

pub struct TauriOverlay {
    pub app: AppHandle,
    pub gate: Arc<Mutex<HideGate>>,
    pub presence: Arc<Mutex<Presence>>,
}

impl OverlaySink for TauriOverlay {
    fn show(&self, s: &OverlayState) {
        let gen = match self.gate.lock() {
            Ok(mut g) => g.on_state(),
            Err(_) => return,
        };
        let hidden = matches!(s, OverlayState::Hidden | OverlayState::Cancelled);
        let fresh = self.presence.lock().map(|mut p| on_pill_state(&mut p, hidden)).unwrap_or(true);
        if matches!(s, OverlayState::Recording { warn: false, .. }) {
            if let Some(t) = self.app.try_state::<OverlayTiming>() {
                if let Ok(mut t) = t.0.lock() {
                    *t = Some(Instant::now());
                }
            }
        }
        let app = self.app.clone();
        let state = s.clone();
        let _ = self.app.run_on_main_thread(move || apply(&app, &state, fresh));
        let delay = match s {
            OverlayState::Hidden => HIDE_AFTER,
            OverlayState::Cancelled => CANCEL_HIDE_AFTER,
            _ => return,
        };
        let app = self.app.clone();
        let gate = self.gate.clone();
        let presence = self.presence.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(delay).await;
            let a = app.clone();
            // Decided on the main thread, in order with the watcher's `show_rest`.
            let _ = app.run_on_main_thread(move || {
                if !gate.lock().map(|g| g.still_current(gen)).unwrap_or(false) {
                    return;
                }
                // With the ears shown the island folds into them and the window stays (§4.2a).
                let gone = presence.lock().map(|mut p| after_fold(&mut p)).unwrap_or(true);
                if gone {
                    hide_window(&a);
                }
            });
        });
    }
}

/// `fresh`: the first state after idle — only then may the window follow the mouse to another display.
fn apply(app: &AppHandle, s: &OverlayState, fresh: bool) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    match s {
        // The window stays up while the island folds away; see `TauriOverlay::show`.
        OverlayState::Hidden | OverlayState::Cancelled => {
            let _ = app.emit_to(LABEL, "overlay://state", s);
            let _ = w.set_ignore_cursor_events(true);
        }
        _ => {
            // The notch size must reach the page before the state that makes the island grow.
            // Mid-recording (limit warning, processing, result) the island never changes display.
            if fresh {
                place(app, &w);
            }
            let _ = app.emit_to(LABEL, "overlay://state", s);
            let clickable = matches!(s, OverlayState::Error { action: Some(_), .. });
            let _ = w.set_ignore_cursor_events(!clickable);
            order_front(&w);
        }
    }
}

/// Shows the ears; `replace` first moves the window to the display under the mouse.
/// Must run on the main thread.
pub fn show_rest(app: &AppHandle, replace: bool) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    if replace {
        place(app, &w);
    }
    let _ = app.emit_to(LABEL, "overlay://rest", serde_json::json!({ "shown": true }));
    order_front(&w);
}

/// Orders the window out. Must run on the main thread.
pub fn hide_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        order_out(&w);
    }
}

/// Top-centre of the screen under the mouse; tells the page the notch size there.
fn place(app: &AppHandle, w: &WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let (Some(m), Ok(size)) = (monitor, w.outer_size()) else { return };
    let (x, y) = island_origin(
        m.position().x as f64,
        m.position().y as f64,
        m.size().width as f64,
        size.width as f64,
    );
    let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    #[cfg(target_os = "macos")]
    let notch = mac::notch(m.position().x as f64, m.scale_factor());
    #[cfg(not(target_os = "macos"))]
    let notch: Option<Notch> = None;
    tracing::info!(x, y, ?notch, "overlay placed");
    let _ = app.emit_to(LABEL, "overlay://screen", serde_json::json!({ "notch": notch }));
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use tauri::WebviewWindow;

    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;

    use super::Notch;

    const NS_POPUP_MENU_WINDOW_LEVEL: isize = 101;
    const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
    const STATIONARY: usize = 1 << 4;
    const IGNORES_CYCLE: usize = 1 << 6;
    const FULL_SCREEN_AUXILIARY: usize = 1 << 8;

    fn ns(w: &WebviewWindow) -> Option<&'static AnyObject> {
        let ptr = w.ns_window().ok()? as *mut AnyObject;
        // SAFETY: Tauri returns a live NSWindow owned by the app for its whole lifetime.
        unsafe { ptr.as_ref() }
    }

    mod class {
        tauri_nspanel::tauri_panel! {
            panel!(OverlayPanel {
                config: {
                    can_become_key_window: false,
                    can_become_main_window: false,
                    is_floating_panel: true
                }
            })
        }
    }

    /// Turns the window into a non-activating NSPanel, so clicking the pill never
    /// activates Utter or steals focus from the app being typed into.
    fn make_panel(w: &WebviewWindow) {
        use tauri_nspanel::{StyleMask, WebviewWindowExt};
        match w.to_panel::<class::OverlayPanel>() {
            Ok(panel) => {
                if let Err(e) = panel.add_style_mask(StyleMask::empty().nonactivating_panel().into()) {
                    tracing::warn!("overlay panel style: {e:?}");
                }
            }
            Err(e) => tracing::warn!("overlay stays a plain window: {e}"),
        }
    }

    pub fn configure(w: &WebviewWindow) {
        make_panel(w);
        // The panel is the same NSWindow object with a new class; configure it as before.
        let Some(win) = ns(w) else { return };
        let behavior = CAN_JOIN_ALL_SPACES | STATIONARY | IGNORES_CYCLE | FULL_SCREEN_AUXILIARY;
        unsafe {
            let _: () = msg_send![win, setLevel: NS_POPUP_MENU_WINDOW_LEVEL];
            let _: () = msg_send![win, setCollectionBehavior: behavior];
            let _: () = msg_send![win, setHidesOnDeactivate: false];
            let _: () = msg_send![win, setIgnoresMouseEvents: true];
        }
    }

    /// The cut-out of the display whose left edge is at `x` (physical pixels), if any.
    /// Must run on the main thread.
    pub fn notch(x: f64, scale: f64) -> Option<Notch> {
        let mtm = MainThreadMarker::new()?;
        let screens = NSScreen::screens(mtm);
        let screen = screens.iter().find(|s| (s.frame().origin.x * scale - x).abs() < 2.0)?;
        let top = screen.safeAreaInsets().top;
        if top <= 0.0 {
            return None;
        }
        let frame = screen.frame();
        let width = frame.size.width - screen.auxiliaryTopLeftArea().size.width - screen.auxiliaryTopRightArea().size.width;
        (width > 0.0).then_some(Notch { width, height: top })
    }

    /// Shows the window without activating Utter, so focus stays in the user's app.
    pub fn order_front(w: &WebviewWindow) {
        if let Some(win) = ns(w) {
            unsafe {
                let _: () = msg_send![win, orderFrontRegardless];
            }
        }
    }

    pub fn order_out(w: &WebviewWindow) {
        if let Some(win) = ns(w) {
            unsafe {
                let nil: *const AnyObject = std::ptr::null();
                let _: () = msg_send![win, orderOut: nil];
            }
        }
    }
}

#[cfg(target_os = "macos")]
use mac::{order_front, order_out};

#[cfg(not(target_os = "macos"))]
fn order_front(w: &WebviewWindow) {
    let _ = w.show();
}

#[cfg(not(target_os = "macos"))]
fn order_out(w: &WebviewWindow) {
    let _ = w.hide();
}

pub fn configure(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if let Some(w) = app.get_webview_window(LABEL) {
        mac::configure(&w);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_origin_is_top_centre() {
        assert_eq!(island_origin(0.0, 0.0, 1512.0, 440.0), (536.0, 0.0));
    }

    #[test]
    fn island_origin_on_second_display() {
        assert_eq!(island_origin(1512.0, -200.0, 1920.0, 440.0), (2252.0, -200.0));
    }

    #[test]
    fn new_state_cancels_pending_hide() {
        let mut gate = HideGate::default();
        let hide = gate.on_state();
        let recording = gate.on_state();
        assert!(!gate.still_current(hide));
        assert!(gate.still_current(recording));
    }
}
