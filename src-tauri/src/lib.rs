pub mod audio;
pub mod app;
pub mod controller;
pub mod history;
pub mod hotkey;
pub mod jobs;
pub mod output;
pub mod pipeline;
pub mod permissions;
pub mod providers;
pub mod recorder;
pub mod secrets;
pub mod settings;
pub mod usage;
pub mod types;

use tauri::Manager;

/// Debug-only: `utter --debug-deliver <text> --method paste|type --delay <secs>`
/// delivers `text` to the focused app and exits. Used by `scripts/paste-matrix.sh`.
#[cfg(all(debug_assertions, target_os = "macos"))]
pub fn debug_deliver(args: &[String]) -> Option<String> {
    let pos = args.iter().position(|a| a == "--debug-deliver")?;
    let text = args.get(pos + 1)?.clone();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1));
    let method = match value("--method").map(String::as_str) {
        Some("type") => settings::InsertMethod::Type,
        _ => settings::InsertMethod::Paste,
    };
    let delay: f64 = value("--delay").and_then(|d| d.parse().ok()).unwrap_or(0.0);
    std::thread::sleep(std::time::Duration::from_secs_f64(delay));
    let caret = {
        use output::InputPlatform;
        output::macos::MacInput.caret_context()
    };
    let outcome = match output::clipboard::ArboardClipboard::new() {
        Ok(mut cb) => output::deliver(&text, method, &mut cb, &output::macos::MacInput, None),
        Err(e) => return Some(format!("clipboard unavailable: {e}")),
    };
    // Synthetic key events can be dropped if the posting process exits at once
    // (measured: 2/5 pastes lost without this pause, 10/10 with it).
    std::thread::sleep(std::time::Duration::from_millis(500));
    Some(format!("{outcome:?} caret={caret:?}"))
}

struct LogGuard(#[allow(dead_code)] Option<tracing_appender::non_blocking::WorkerGuard>);

fn init_core(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::Arc;

    use app::commands::AppState;
    use app::orchestrator::{Deps, Orchestrator, Shared};
    use app::services::{HotkeyService, HttpProviders, SystemOutput, SystemPermissions};
    use tauri::Emitter;

    let data_dir = app.path().app_data_dir()?;
    let log_dir = app.path().app_log_dir()?;
    app.manage(LogGuard(app::diagnostics::init_logging(&log_dir)));
    app::diagnostics::install_panic_hook();
    tracing::info!("Utter {} starting", env!("CARGO_PKG_VERSION"));

    let lang = sys_locale::get_locale();
    let settings = settings::load(&data_dir.join("settings.json"), lang.as_deref());
    app::tray::build(app.handle(), &settings)?;
    let shared = Arc::new(Shared::new(settings.clone(), data_dir));
    shared.remove_orphan_audio();

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let (level_tx, mut level_rx) = tokio::sync::watch::channel(0.0f32);
    let recorder = Arc::new(recorder::Recorder::new(level_tx));
    let secrets: Arc<dyn secrets::SecretStore> = Arc::new(secrets::KeychainStore::new());
    let providers: Arc<dyn app::orchestrator::ProviderFactory> = Arc::new(HttpProviders::default());
    let hotkeys = Arc::new(HotkeyService::new(settings.hotkeys.clone(), tx.clone()));

    app.manage(app::overlay::OverlayTiming::default());
    app.manage(app::updates::Updates::default());
    app::updates::spawn(app.handle().clone());
    app::overlay::configure(app.handle());
    let gate: Arc<std::sync::Mutex<app::overlay::HideGate>> = Default::default();
    let presence: Arc<std::sync::Mutex<app::overlay_rest::Presence>> = Default::default();
    let rest_wake = Arc::new(tokio::sync::Notify::new());
    app.manage(app::overlay_rest::RestWake(rest_wake.clone()));
    app.manage(app::overlay_rest::RestPresence(presence.clone()));
    let deps = Deps {
        secrets: secrets.clone(),
        audio: recorder,
        providers: providers.clone(),
        output: Arc::new(SystemOutput::default()),
        overlay: Arc::new(app::overlay::TauriOverlay {
            app: app.handle().clone(),
            gate: gate.clone(),
            presence: presence.clone(),
        }),
        permissions: Arc::new(SystemPermissions),
        hotkeys: hotkeys.clone(),
        ui: Arc::new(app::services::TauriUi { app: app.handle().clone() }),
    };
    let orchestrator = Orchestrator::new(deps, shared.clone(), tx.clone());
    app::overlay_rest::spawn_watcher(app.handle().clone(), shared.clone(), presence, gate, rest_wake);
    tauri::async_runtime::spawn(orchestrator.run(rx));

    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        while level_rx.changed().await.is_ok() {
            let level = *level_rx.borrow_and_update();
            let _ = handle.emit_to("overlay", "overlay://level", level);
        }
    });

    // The login item may have been removed outside Utter; the setting is the source of truth.
    {
        use tauri_plugin_autostart::ManagerExt;
        if app.autolaunch().is_enabled().ok() != Some(settings.autostart) {
            app::commands::set_autostart(app.handle(), settings.autostart);
        }
    }
    app.manage(AppState { shared, tx, secrets: secrets.clone(), hotkeys, providers, log_dir });
    #[cfg(feature = "debug-eval")]
    app::debug_eval::start(app.handle().clone(), app.path().app_data_dir()?);
    // Reading the key may show a Keychain prompt; never wait for it on the main thread
    // (it froze the tray and hotkeys for as long as the prompt was open).
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        let route = app::settings_window::start_route(
            settings.onboarding_done,
            secrets.get(settings.provider).is_some(),
            permissions::accessibility(false),
        );
        if let Some(r) = route {
            let h = handle.clone();
            let _ = handle.run_on_main_thread(move || app::settings_window::open(&h, Some(r)));
        }
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());
    builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            app::settings_window::open(app, None);
        }))
        .invoke_handler(tauri::generate_handler![
            app::commands::get_settings,
            app::commands::save_settings,
            app::commands::set_api_key,
            app::commands::api_key_mask,
            app::commands::clipboard_key,
            app::commands::verify_key,
            app::commands::capture_key,
            app::commands::cancel_capture,
            app::commands::set_trial,
            app::commands::free_limits,
            app::commands::permissions_status,
            app::commands::request_microphone,
            app::commands::open_accessibility_settings,
            app::commands::history_list,
            app::commands::history_retry,
            app::commands::usage_today,
            app::commands::copy_diagnostics,
            app::commands::app_info,
            app::commands::overlay_action,
            app::commands::settings_ready,
            app::commands::open_settings,
            app::commands::open_link,
            app::commands::copy_text,
            app::commands::open_microphone_settings,
            app::commands::overlay_painted,
            app::commands::overlay_ready,
            app::commands::update_state,
            app::commands::check_updates,
            app::commands::install_update,
            app::commands::overlay_frames,
            app::commands::ui_log,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            init_core(app)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        // The app lives in the tray: closing the last window must not quit it.
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
            // macOS does not start a second copy on a repeated launch; it sends "reopen".
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => app::settings_window::open(app, None),
            _ => {}
        });
}
