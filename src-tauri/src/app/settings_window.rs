//! The settings window: created on demand and destroyed on close, so its WebKit
//! process only exists while the user is looking at it.

use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use super::orchestrator::Shared;
use super::services::HotkeyService;
use crate::settings::Theme;

pub const LABEL: &str = "settings";
/// Shown at the latest after this long, even if the page never reported ready.
const SHOW_FALLBACK: Duration = Duration::from_millis(1500);

/// Window background before the page paints (spec §5.1 `bg`), so there is no white flash.
pub fn background(theme: Theme, system_dark: bool) -> (u8, u8, u8) {
    let dark = match theme {
        Theme::Dark => true,
        Theme::Light => false,
        Theme::System => system_dark,
    };
    if dark {
        (0x0A, 0x0A, 0x0A)
    } else {
        (0xFA, 0xFA, 0xF9)
    }
}

/// The only external pages the UI may open.
pub fn link_url(id: &str) -> Option<&'static str> {
    Some(match id {
        "gemini_key" => "https://aistudio.google.com/apikey",
        "groq_key" => "https://console.groq.com/keys",
        "openai_key" => "https://platform.openai.com/api-keys",
        "github" => "https://github.com/Gattapelatta/utter",
        "releases" => "https://github.com/Gattapelatta/utter/releases",
        _ => return None,
    })
}

/// Where the window opens on launch, if it should open at all.
pub fn start_route(onboarding_done: bool, has_key: bool, accessibility: bool) -> Option<&'static str> {
    if !onboarding_done {
        Some("onboarding")
    } else if !has_key {
        Some("provider")
    } else if !accessibility {
        Some("permissions")
    } else {
        None
    }
}

/// Closing the window must not leave a key capture or the onboarding trial running.
pub fn on_destroyed(shared: &Shared, hotkeys: &HotkeyService) {
    shared.trial.store(false, Ordering::SeqCst);
    hotkeys.cancel_capture();
}

/// Opens the window at `route` (or brings the existing one forward and navigates).
pub fn open(app: &AppHandle, route: Option<&str>) {
    tracing::info!(?route, "open settings");
    if let Some(w) = app.get_webview_window(LABEL) {
        if let Some(r) = route {
            let _ = app.emit_to(LABEL, "ui://navigate", r);
        }
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let state = app.try_state::<super::commands::AppState>();
    let theme = state.as_ref().map(|s| s.shared.settings().theme).unwrap_or(Theme::Dark);
    let system_dark = app
        .get_webview_window("overlay")
        .and_then(|w| w.theme().ok())
        .is_none_or(|t| t == tauri::Theme::Dark);
    let (r, g, b) = background(theme, system_dark);
    let url = format!("settings.html#/{}", route.unwrap_or("general"));
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(url.into()))
        .title("Utter")
        .inner_size(900.0, 620.0)
        .min_inner_size(760.0, 520.0)
        .visible(false)
        .background_color(tauri::window::Color(r, g, b, 255));
    #[cfg(target_os = "macos")]
    let built = built.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    match built.build() {
        Ok(w) => {
            tracing::info!(?route, "settings window created");
            let app2 = app.clone();
            w.on_window_event(move |e| {
                if let tauri::WindowEvent::Destroyed = e {
                    if let Some(s) = app2.try_state::<super::commands::AppState>() {
                        on_destroyed(&s.shared, &s.hotkeys);
                    }
                }
            });
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(SHOW_FALLBACK).await;
                reveal(&app2);
            });
        }
        Err(e) => tracing::warn!("settings window: {e}"),
    }
}

/// Shows the window once the page has painted (or the fallback timer fired).
pub fn reveal(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        if !w.is_visible().unwrap_or(true) {
            tracing::info!("settings window shown");
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_settings_turns_trial_off() {
        let dir = tempfile::tempdir().unwrap();
        let shared = Shared::new(Default::default(), dir.path().to_path_buf());
        shared.trial.store(true, Ordering::SeqCst);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let hotkeys = HotkeyService::new(crate::settings::Settings::default().hotkeys, tx);
        on_destroyed(&shared, &hotkeys);
        assert!(!shared.trial.load(Ordering::SeqCst));
    }

    #[test]
    fn unknown_link_is_rejected() {
        assert_eq!(link_url("evil"), None);
        assert_eq!(link_url("https://evil.example"), None);
        assert_eq!(link_url("gemini_key"), Some("https://aistudio.google.com/apikey"));
    }

    #[test]
    fn window_background_matches_theme() {
        assert_eq!(background(Theme::Dark, false), (0x0A, 0x0A, 0x0A));
        assert_eq!(background(Theme::Light, true), (0xFA, 0xFA, 0xF9));
        assert_eq!(background(Theme::System, true), (0x0A, 0x0A, 0x0A));
        assert_eq!(background(Theme::System, false), (0xFA, 0xFA, 0xF9));
    }

    #[test]
    fn start_route_follows_setup_order() {
        assert_eq!(start_route(false, false, false), Some("onboarding"));
        assert_eq!(start_route(true, false, false), Some("provider"));
        assert_eq!(start_route(true, true, false), Some("permissions"));
        assert_eq!(start_route(true, true, true), None);
    }
}
