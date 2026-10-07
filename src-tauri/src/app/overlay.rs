//! The recording pill: a transparent, click-through, non-activating panel.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

use super::orchestrator::{OverlaySink, OverlayState};

const LABEL: &str = "overlay";
/// Window bottom above the work area's bottom edge, in points. The pill sits 24 pt
/// higher inside the window (room for its shadow), i.e. 48 pt above the edge (§4.2).
const BOTTOM_MARGIN: f64 = 24.0;
/// The window is ordered out only after the pill's exit animation (160 ms / 120 ms).
const HIDE_AFTER: Duration = Duration::from_millis(200);
const CANCEL_HIDE_AFTER: Duration = Duration::from_millis(140);

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
}

impl OverlaySink for TauriOverlay {
    fn show(&self, s: &OverlayState) {
        let gen = match self.gate.lock() {
            Ok(mut g) => g.on_state(),
            Err(_) => return,
        };
        if matches!(s, OverlayState::Recording { warn: false, .. }) {
            if let Some(t) = self.app.try_state::<OverlayTiming>() {
                if let Ok(mut t) = t.0.lock() {
                    *t = Some(Instant::now());
                }
            }
        }
        let app = self.app.clone();
        let state = s.clone();
        let _ = self.app.run_on_main_thread(move || apply(&app, &state));
        let delay = match s {
            OverlayState::Hidden => HIDE_AFTER,
            OverlayState::Cancelled => CANCEL_HIDE_AFTER,
            _ => return,
        };
        let app = self.app.clone();
        let gate = self.gate.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(delay).await;
            let current = gate.lock().map(|g| g.still_current(gen)).unwrap_or(false);
            if current {
                let a = app.clone();
                let _ = app.run_on_main_thread(move || {
                    if let Some(w) = a.get_webview_window(LABEL) {
                        order_out(&w);
                    }
                });
            }
        });
    }
}

fn apply(app: &AppHandle, s: &OverlayState) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    let _ = app.emit_to(LABEL, "overlay://state", s);
    match s {
        // The window stays up while the pill animates out; see `TauriOverlay::show`.
        OverlayState::Hidden | OverlayState::Cancelled => {
            let _ = w.set_ignore_cursor_events(true);
        }
        _ => {
            place(app, &w);
            let clickable = matches!(s, OverlayState::Error { action: Some(_), .. });
            let _ = w.set_ignore_cursor_events(!clickable);
            order_front(&w);
        }
    }
}

/// Bottom-centre of the screen under the mouse.
fn place(app: &AppHandle, w: &WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let (Some(m), Ok(size)) = (monitor, w.outer_size()) else { return };
    let area = m.work_area();
    let x = area.position.x + (area.size.width as i32 - size.width as i32) / 2;
    let y = area.position.y + area.size.height as i32
        - size.height as i32
        - (BOTTOM_MARGIN * m.scale_factor()) as i32;
    let _ = w.set_position(PhysicalPosition::new(x, y));
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use tauri::WebviewWindow;

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
    fn new_state_cancels_pending_hide() {
        let mut gate = HideGate::default();
        let hide = gate.on_state();
        let recording = gate.on_state();
        assert!(!gate.still_current(hide));
        assert!(gate.still_current(recording));
    }
}
