//! Tauri commands used by the settings window and the overlay.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc::UnboundedSender;

use super::diagnostics;
use super::settings_window;
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

/// Whether the login item must be switched on (`Some(true)`) or off for this change.
pub fn autostart_change(prev: &Settings, next: &Settings) -> Option<bool> {
    (prev.autostart != next.autostart).then_some(next.autostart)
}

/// The one path for every settings change (window, tray, onboarding): saves, applies,
/// and broadcasts `ui://settings` so every window shows the same values.
pub fn update_settings(app: &AppHandle, state: &AppState, next: Settings) -> Result<(), String> {
    let path = state.shared.data_dir.join("settings.json");
    let prev = state.shared.settings();
    // Connect or disconnect the agents first: Codex's previous `notify` is kept in the settings.
    let next = if prev.agents != next.agents { crate::agents::service::reconcile(next) } else { next };
    settings::save(&path, &next).map_err(|e| e.to_string())?;
    if let Some(on) = autostart_change(&prev, &next) {
        set_autostart(app, on);
    }
    if !next.history_enabled {
        if let Ok(mut h) = state.shared.history.lock() {
            for f in h.clear() {
                let _ = std::fs::remove_file(f);
            }
            let _ = h.save();
        }
    }
    if let Ok(mut s) = state.shared.settings.lock() {
        *s = next.clone();
    }
    let _ = state.tx.send(Input::SettingsChanged);
    if let Some(w) = app.get_webview_window(settings_window::LABEL) {
        let _ = w.set_theme(settings_window::window_theme(next.theme));
    }
    super::tray::refresh(app, &next);
    if let Some(w) = app.try_state::<super::overlay_rest::RestWake>() {
        w.0.notify_one();
    }
    let _ = app.emit("ui://settings", &next);
    if prev.agents != next.agents {
        // The ears show or drop the agents' colours at once.
        crate::agents::service::publish(app);
    }
    Ok(())
}

/// Registers or removes the login item (a LaunchAgent on macOS).
pub fn set_autostart(app: &AppHandle, on: bool) {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    let r = if on { launcher.enable() } else { launcher.disable() };
    if let Err(e) = r {
        tracing::warn!("autostart {on}: {e}");
    }
}

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<AppState>, settings: Settings) -> Result<(), String> {
    update_settings(&app, &state, settings)
}

/// The settings page has painted its first frame: show the window without a flash.
#[tauri::command]
pub fn settings_ready(app: AppHandle) {
    settings_window::reveal(&app);
}

#[tauri::command]
pub fn open_settings(app: AppHandle, route: Option<String>) {
    settings_window::open(&app, route.as_deref());
}

/// Opens one of the fixed external links; anything else is refused.
#[tauri::command]
pub fn open_link(link: String) -> Result<(), String> {
    let url = settings_window::link_url(&link).ok_or("unknown_link")?;
    open_url(url);
    Ok(())
}

#[tauri::command]
pub fn copy_text(text: String) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_microphone_settings() {
    open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
}

fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = url;
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

/// The clipboard's text, but only if it looks like a key of `provider` (onboarding picks the
/// key up by itself when the user comes back from the provider's page).
#[tauri::command]
pub fn clipboard_key(provider: ProviderKind) -> Option<String> {
    let text = arboard::Clipboard::new().ok()?.get_text().ok()?;
    crate::secrets::looks_like_key(provider, &text).then(|| text.trim().to_string())
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

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub os: String,
}

/// What a person calls their processor ("Apple Silicon"), from Rust's architecture name.
pub fn arch_label(arch: &str) -> &str {
    match arch {
        "aarch64" => "Apple Silicon",
        "x86_64" => "Intel",
        other => other,
    }
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    #[cfg(target_os = "macos")]
    let os = {
        let v = objc2_foundation::NSProcessInfo::processInfo().operatingSystemVersion();
        let patch = if v.patchVersion > 0 { format!(".{}", v.patchVersion) } else { String::new() };
        format!("macOS {}.{}{patch} · {}", v.majorVersion, v.minorVersion, arch_label(std::env::consts::ARCH))
    };
    #[cfg(not(target_os = "macos"))]
    let os = format!("{} · {}", std::env::consts::OS, arch_label(std::env::consts::ARCH));
    AppInfo { version: env!("CARGO_PKG_VERSION").to_string(), os }
}

#[tauri::command]
pub fn copy_diagnostics(state: State<AppState>) -> String {
    diagnostics::report(&state.shared.settings(), &state.log_dir)
}

/// Clicking an error pill opens the settings window at the section that fixes it.
#[tauri::command]
pub fn overlay_action(app: AppHandle, action: String) {
    let route = match action.as_str() {
        "open_provider" => Some("provider"),
        "open_permissions" => Some("permissions"),
        _ => None,
    };
    settings_window::open(&app, route);
}

/// The pill's first painted frame of a recording (budget §8.4: < 50 ms).
#[tauri::command]
pub fn update_state(updates: State<super::updates::Updates>) -> super::updates::UpdateState {
    updates.state()
}

#[tauri::command]
pub fn check_updates(app: AppHandle) {
    tauri::async_runtime::spawn(super::updates::check(app));
}

/// Installs only on the user's press (§3.7); restarts Utter when done.
#[tauri::command]
pub fn install_update(app: AppHandle) {
    tauri::async_runtime::spawn(super::updates::install(app));
}

/// The overlay page has loaded its listeners; earlier rest events were lost.
#[tauri::command]
pub fn overlay_ready(
    presence: State<super::overlay_rest::RestPresence>,
    wake: State<super::overlay_rest::RestWake>,
) {
    super::overlay_rest::page_ready(&presence.0, &wake.0);
}

#[tauri::command]
pub fn overlay_painted(timing: State<super::overlay::OverlayTiming>) {
    if let Some(t) = timing.0.lock().ok().and_then(|mut t| t.take()) {
        tracing::info!(painted_ms = t.elapsed().as_millis() as u64, "timing: show→pill painted");
    }
}

/// Script errors and milestones from the settings page, so problems on the user's Mac
/// show up in the app log (the page itself is invisible to diagnostics otherwise).
#[tauri::command]
pub fn ui_log(level: String, message: String) {
    let message: String = message.chars().take(2_000).collect();
    if level == "error" {
        tracing::warn!("settings page: {message}");
    } else {
        tracing::info!("settings page: {message}");
    }
}

/// Frame intervals of the character animation during one recording (smoothness, §8.4).
#[tauri::command]
pub fn overlay_frames(avg: f64, p95: f64, max: f64, count: u32) {
    tracing::info!(avg_ms = avg, p95_ms = p95, max_ms = max, frames = count, "timing: overlay frames while listening");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processor_names_are_human() {
        assert_eq!(arch_label("aarch64"), "Apple Silicon");
        assert_eq!(arch_label("x86_64"), "Intel");
        assert_eq!(arch_label("riscv64"), "riscv64");
    }

    #[test]
    fn autostart_follows_the_switch_only_when_it_changes() {
        let off = Settings::default();
        let on = Settings { autostart: true, ..Settings::default() };
        assert_eq!(autostart_change(&off, &on), Some(true));
        assert_eq!(autostart_change(&on, &off), Some(false));
        assert_eq!(autostart_change(&on, &on), None);
        let themed = Settings { theme: crate::settings::Theme::Light, ..off.clone() };
        assert_eq!(autostart_change(&off, &themed), None);
    }
}
