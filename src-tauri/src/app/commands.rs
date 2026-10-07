//! Tauri commands used by the settings window and the overlay.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tokio::sync::mpsc::UnboundedSender;

use super::diagnostics;
use super::orchestrator::{Input, ProviderFactory, Shared};
use super::services::HotkeyService;
use crate::history::HistoryEntry;
use crate::permissions::{self, PermStatus};
use crate::providers::defaults::{self, FreeLimits};
use crate::providers::ProviderError;
use crate::secrets::{mask, SecretStore};
use crate::settings::{self, ProviderKind, Settings};
use crate::types::KeyId;
use crate::usage::UsageDay;

pub struct AppState {
    pub shared: Arc<Shared>,
    pub tx: UnboundedSender<Input>,
    pub secrets: Arc<dyn SecretStore>,
    pub hotkeys: Arc<HotkeyService>,
    pub providers: Arc<dyn ProviderFactory>,
    pub log_dir: PathBuf,
}

#[derive(Serialize)]
pub struct PermissionsStatus {
    pub microphone: PermStatus,
    pub accessibility: bool,
    pub hotkeys_running: bool,
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.shared.settings()
}

#[tauri::command]
pub fn save_settings(state: State<AppState>, settings: Settings) -> Result<(), String> {
    let path = state.shared.data_dir.join("settings.json");
    settings::save(&path, &settings).map_err(|e| e.to_string())?;
    if !settings.history_enabled {
        if let Ok(mut h) = state.shared.history.lock() {
            for f in h.clear() {
                let _ = std::fs::remove_file(f);
            }
            let _ = h.save();
        }
    }
    if let Ok(mut s) = state.shared.settings.lock() {
        *s = settings;
    }
    let _ = state.tx.send(Input::SettingsChanged);
    Ok(())
}

#[tauri::command]
pub fn set_api_key(state: State<AppState>, provider: ProviderKind, key: String) -> Result<(), String> {
    let r = if key.trim().is_empty() {
        state.secrets.delete(provider)
    } else {
        state.secrets.set(provider, &key)
    };
    let _ = state.tx.send(Input::SettingsChanged);
    r.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn api_key_mask(state: State<AppState>, provider: ProviderKind) -> Option<String> {
    state.secrets.get(provider).map(|k| mask(&k))
}

/// Returns an error key the UI can translate: `no_key`, `bad_key`, `network`, `rate_limited`, `service_down`.
#[tauri::command]
pub async fn verify_key(state: State<'_, AppState>, provider: ProviderKind) -> Result<(), String> {
    let key = state.secrets.get(provider).ok_or("no_key")?;
    let p = state.providers.make(provider, key);
    p.verify_key().await.map_err(|e| match e {
        ProviderError::Unauthorized => "bad_key".to_string(),
        ProviderError::Network | ProviderError::Timeout => "network".into(),
        ProviderError::RateLimited { .. } => "rate_limited".into(),
        ProviderError::Server(_) | ProviderError::BadResponse(_) => "service_down".into(),
    })
}

/// Errors: `hotkeys_unavailable`, `cancelled`, `timeout`, `not_assignable`.
#[tauri::command]
pub async fn capture_key(state: State<'_, AppState>) -> Result<KeyId, String> {
    let rx = state.hotkeys.capture_next_key().ok_or("hotkeys_unavailable")?;
    match tokio::time::timeout(Duration::from_secs(15), rx).await {
        Ok(Ok(key)) if key.is_assignable() => Ok(key),
        Ok(Ok(_)) => Err("not_assignable".into()),
        Ok(Err(_)) => Err("cancelled".into()),
        Err(_) => {
            // Otherwise the next hotkey press would still be taken by the capture.
            state.hotkeys.cancel_capture();
            Err("timeout".into())
        }
    }
}

#[tauri::command]
pub fn cancel_capture(state: State<AppState>) {
    state.hotkeys.cancel_capture();
}

/// Onboarding trial: while on, results are shown in the window instead of pasted.
#[tauri::command]
pub fn set_trial(state: State<AppState>, on: bool) {
    state.shared.trial.store(on, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command]
pub fn free_limits(provider: ProviderKind) -> Option<FreeLimits> {
    defaults::free_limits(provider)
}

#[tauri::command]
pub fn permissions_status(state: State<AppState>) -> PermissionsStatus {
    PermissionsStatus {
        microphone: permissions::microphone(),
        accessibility: permissions::accessibility(false),
        hotkeys_running: state.hotkeys.is_running(),
    }
}

#[tauri::command]
pub async fn request_microphone() -> bool {
    permissions::request_microphone().await
}

#[tauri::command]
pub fn open_accessibility_settings() {
    // Prompting once adds Utter to the Accessibility list so the user only flips a switch.
    permissions::accessibility(true);
    permissions::open_accessibility_settings();
}

#[tauri::command]
pub fn history_list(state: State<AppState>) -> Vec<HistoryEntry> {
    state.shared.history.lock().map(|h| h.entries().iter().cloned().collect()).unwrap_or_default()
}

#[tauri::command]
pub fn history_retry(state: State<AppState>, id: u64) {
    let _ = state.tx.send(Input::Retry(id));
}

#[tauri::command]
pub fn usage_today(state: State<AppState>) -> Option<UsageDay> {
    let today = chrono::Local::now().date_naive();
    state.shared.usage.lock().ok().map(|u| u.today(today))
}

#[tauri::command]
pub fn copy_diagnostics(state: State<AppState>) -> String {
    diagnostics::report(&state.shared.settings(), &state.log_dir)
}

/// Clicking an error pill opens the settings window.
#[tauri::command]
pub fn overlay_action(app: AppHandle, _action: String) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}
