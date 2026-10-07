//! Global hotkeys. Only hotkey transitions, Escape, and "another key while a
//! hotkey is held" ever leave this module — never what the user types.

pub mod router;
#[cfg(target_os = "macos")]
pub mod macos;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};

use crate::controller::HotkeyEvent;
use crate::settings::Hotkeys;
use crate::types::KeyId;
use router::Router;

#[derive(thiserror::Error, Debug)]
pub enum HotkeyError {
    #[error("accessibility permission missing")]
    NoPermission,
    #[error("event tap: {0}")]
    Tap(String),
}

pub struct HotkeyHandle {
    router: Arc<Mutex<Router>>,
    /// Platform run loop of the listener thread (CFRunLoopRef on macOS).
    run_loop: Arc<AtomicUsize>,
}

impl HotkeyHandle {
    pub fn set_bindings(&self, b: Hotkeys) {
        if let Ok(mut r) = self.router.lock() {
            r.set_bindings(b);
        }
    }

    pub fn set_recording(&self, on: bool) {
        if let Ok(mut r) = self.router.lock() {
            r.set_recording(on);
        }
    }

    /// Resolves with the next key the user presses; dropped (error) on Escape.
    pub fn capture_next_key(&self) -> oneshot::Receiver<KeyId> {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut r) = self.router.lock() {
            r.begin_capture(tx);
        }
        rx
    }

    pub fn cancel_capture(&self) {
        if let Ok(mut r) = self.router.lock() {
            r.cancel_capture();
        }
    }

    pub fn stop(&self) {
        let rl = self.run_loop.swap(0, Ordering::SeqCst);
        #[cfg(target_os = "macos")]
        macos::stop_run_loop(rl);
        #[cfg(not(target_os = "macos"))]
        let _ = rl;
    }
}

impl Drop for HotkeyHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn start(
    bindings: Hotkeys,
    tx: mpsc::UnboundedSender<HotkeyEvent>,
) -> Result<HotkeyHandle, HotkeyError> {
    let router = Arc::new(Mutex::new(Router::new(bindings)));
    let run_loop = Arc::new(AtomicUsize::new(0));
    #[cfg(target_os = "macos")]
    macos::spawn_tap(router.clone(), tx, run_loop.clone())?;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = tx;
        return Err(HotkeyError::Tap("not implemented on this platform yet".into()));
    }
    #[allow(unreachable_code)]
    Ok(HotkeyHandle { router, run_loop })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs Accessibility / Input Monitoring for the test process"]
    fn tap_starts_and_stops() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let h = start(
            Hotkeys { dictation: KeyId::RightAlt, translate: KeyId::RightMeta },
            tx,
        )
        .expect("event tap");
        h.stop();
    }
}
