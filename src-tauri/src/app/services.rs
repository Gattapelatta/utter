//! Real (OS-backed) implementations of the orchestrator's dependencies.

use std::sync::{Arc, Mutex};

use tokio::sync::mpsc::UnboundedSender;

use tauri::{AppHandle, Emitter};

use super::orchestrator::{
    Deliverer, HotkeyControl, Input, PermissionCheck, Provider, ProviderFactory, UiEvent, UiSink,
};
use crate::hotkey::{self, HotkeyHandle};
use crate::output::{self, DeliveryOutcome};
use crate::permissions::{self, PermStatus};
use crate::pipeline::routes::Route;
use crate::providers::{defaults, gemini_http_client, GeminiClient, OpenAiCompatible, ProviderError};
use crate::settings::{Hotkeys, InsertMethod, ProviderKind};
use crate::types::KeyId;

/// Sends orchestrator events to the settings window (when open).
pub struct TauriUi {
    pub app: AppHandle,
}

impl UiSink for TauriUi {
    fn emit(&self, e: UiEvent) {
        const WINDOW: &str = "settings";
        let _ = match e {
            UiEvent::Status(s) => {
                super::tray::set_status(&self.app, s);
                Ok(())
            }
            UiEvent::Hotkey { mode, down } => {
                self.app.emit_to(WINDOW, "ui://hotkey", serde_json::json!({ "mode": mode, "down": down }))
            }
            UiEvent::Trial(o) => self.app.emit_to(WINDOW, "ui://trial", o),
            UiEvent::HistoryChanged => self.app.emit_to(WINDOW, "ui://history", ()),
        };
    }
}

#[async_trait::async_trait]
impl Provider for OpenAiCompatible {
    async fn prewarm(&self) {
        OpenAiCompatible::prewarm(self).await
    }
    async fn verify_key(&self) -> Result<(), ProviderError> {
        OpenAiCompatible::verify_key(self).await
    }
}

#[async_trait::async_trait]
impl Provider for GeminiClient {
    async fn prewarm(&self) {
        GeminiClient::prewarm(self).await
    }
    async fn verify_key(&self) -> Result<(), ProviderError> {
        GeminiClient::verify_key(self).await
    }
    fn direct(&self) -> Option<&dyn crate::providers::AudioLlm> {
        Some(self)
    }
}

/// Gemini clients share one connection pool, so a backup route starts on a warm connection.
/// After sleep the pool is replaced: a connection can be silently dead after a VPN or network
/// change, and every route would hang on it (review 2026-11-07; Plan 1: new client on wake).
pub struct HttpProviders {
    gemini_http: std::sync::Mutex<reqwest::Client>,
}

impl Default for HttpProviders {
    fn default() -> Self {
        Self { gemini_http: std::sync::Mutex::new(gemini_http_client()) }
    }
}

impl HttpProviders {
    fn gemini_http(&self) -> reqwest::Client {
        self.gemini_http.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl ProviderFactory for HttpProviders {
    fn make(&self, kind: ProviderKind, key: String) -> Arc<dyn Provider> {
        let cfg = defaults::config_for(kind, key);
        match kind {
            ProviderKind::Gemini => Arc::new(GeminiClient::with_client(cfg, self.gemini_http())),
            ProviderKind::Groq | ProviderKind::OpenAi => Arc::new(OpenAiCompatible::new(cfg)),
        }
    }

    fn make_route(&self, kind: ProviderKind, key: String, route: Route) -> Arc<dyn Provider> {
        match kind {
            ProviderKind::Gemini => {
                Arc::new(GeminiClient::with_client(defaults::route_config(kind, key, route), self.gemini_http()))
            }
            ProviderKind::Groq | ProviderKind::OpenAi => self.make(kind, key),
        }
    }

    fn reset(&self) {
        let fresh = gemini_http_client();
        *self.gemini_http.lock().unwrap_or_else(|e| e.into_inner()) = fresh;
    }
}

/// Delivers text and remembers the last insertion for smart spacing.
#[derive(Default)]
pub struct SystemOutput {
    recent: Mutex<Option<output::spacing::RecentInsert>>,
}

impl Deliverer for SystemOutput {
    fn deliver(&self, text: &str, method: InsertMethod) -> DeliveryOutcome {
        let mut cb = match output::clipboard::ArboardClipboard::new() {
            Ok(cb) => cb,
            Err(e) => {
                tracing::error!("clipboard unavailable: {e}");
                return DeliveryOutcome::CopiedOnly(output::CopyReason::ClipboardFailed);
            }
        };
        #[cfg(target_os = "macos")]
        let outcome = {
            use output::InputPlatform;
            let input = output::macos::MacInput;
            let pid = input.frontmost_pid();
            let now = std::time::Instant::now();
            let recent = self.recent.lock().ok().and_then(|r| *r);
            let fallback = output::spacing::fallback_prev(recent.as_ref(), pid, now);
            let outcome = output::deliver(text, method, &mut cb, &input, fallback);
            if matches!(outcome, DeliveryOutcome::Pasted | DeliveryOutcome::Typed) {
                if let (Some(pid), Some(last), Ok(mut r)) = (pid, text.chars().last(), self.recent.lock()) {
                    *r = Some(output::spacing::RecentInsert { pid, at: now, last });
                }
            }
            outcome
        };
        #[cfg(not(target_os = "macos"))]
        let outcome = {
            let _ = (method, &mut cb, text);
            DeliveryOutcome::CopiedOnly(output::CopyReason::InputFailed)
        };
        tracing::info!("delivery outcome: {outcome:?}");
        outcome
    }
}

pub struct SystemPermissions;

impl PermissionCheck for SystemPermissions {
    fn accessibility(&self) -> bool {
        permissions::accessibility(false)
    }
    fn microphone(&self) -> PermStatus {
        permissions::microphone()
    }
}

/// Owns the global keyboard listener and restarts it on demand.
pub struct HotkeyService {
    handle: Mutex<Option<HotkeyHandle>>,
    bindings: Mutex<Hotkeys>,
    tx: UnboundedSender<Input>,
}

impl HotkeyService {
    pub fn new(bindings: Hotkeys, tx: UnboundedSender<Input>) -> Self {
        Self { handle: Mutex::new(None), bindings: Mutex::new(bindings), tx }
    }

    pub fn capture_next_key(&self) -> Option<tokio::sync::oneshot::Receiver<KeyId>> {
        self.handle.lock().ok()?.as_ref().map(|h| h.capture_next_key())
    }

    pub fn cancel_capture(&self) {
        if let Ok(h) = self.handle.lock() {
            if let Some(h) = h.as_ref() {
                h.cancel_capture();
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.handle.lock().map(|h| h.is_some()).unwrap_or(false)
    }
}

impl HotkeyControl for HotkeyService {
    fn set_bindings(&self, b: Hotkeys) {
        if let Ok(mut cur) = self.bindings.lock() {
            *cur = b.clone();
        }
        if let Ok(h) = self.handle.lock() {
            if let Some(h) = h.as_ref() {
                h.set_bindings(b);
            }
        }
    }

    fn set_recording(&self, on: bool) {
        if let Ok(h) = self.handle.lock() {
            if let Some(h) = h.as_ref() {
                h.set_recording(on);
            }
        }
    }

    fn ensure_running(&self) -> bool {
        let Ok(mut slot) = self.handle.lock() else { return false };
        if slot.is_some() {
            return true;
        }
        if !permissions::accessibility(false) {
            return false;
        }
        let bindings = self.bindings.lock().map(|b| b.clone()).unwrap_or_else(|_| crate::settings::Settings::default().hotkeys);
        let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel();
        match hotkey::start(bindings, ev_tx) {
            Ok(h) => {
                let tx = self.tx.clone();
                tauri::async_runtime::spawn(async move {
                    while let Some(ev) = ev_rx.recv().await {
                        if tx.send(Input::Hotkey(ev)).is_err() {
                            break;
                        }
                    }
                });
                tracing::info!("hotkey listener started");
                *slot = Some(h);
                true
            }
            Err(e) => {
                tracing::warn!("hotkey listener failed: {e}");
                false
            }
        }
    }

    fn restart(&self) {
        if let Ok(mut slot) = self.handle.lock() {
            slot.take();
        }
        self.ensure_running();
    }
}
