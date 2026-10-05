//! The recording pill: a transparent, click-through window that never takes focus.

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

use super::orchestrator::{OverlaySink, OverlayState};

const LABEL: &str = "overlay";
/// Distance from the bottom of the usable screen area, in points.
const BOTTOM_MARGIN: f64 = 48.0;

pub struct TauriOverlay {
    pub app: AppHandle,
}

impl OverlaySink for TauriOverlay {
    fn show(&self, s: &OverlayState) {
        let app = self.app.clone();
        let s = s.clone();
        let _ = self.app.run_on_main_thread(move || apply(&app, &s));
    }
}

fn apply(app: &AppHandle, s: &OverlayState) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    let _ = app.emit_to(LABEL, "overlay://state", s);
    match s {
        OverlayState::Hidden => order_out(&w),
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

    pub fn configure(w: &WebviewWindow) {
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
