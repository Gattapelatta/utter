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
pub mod sounds;
pub mod usage;
pub mod types;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

fn show_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

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
    let shared = Arc::new(Shared::new(settings.clone(), data_dir));
    shared.remove_orphan_audio();

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let (level_tx, mut level_rx) = tokio::sync::watch::channel(0.0f32);
    let recorder = Arc::new(recorder::Recorder::new(level_tx));
    let secrets: Arc<dyn secrets::SecretStore> = Arc::new(secrets::KeychainStore::new());
    let providers: Arc<dyn app::orchestrator::ProviderFactory> = Arc::new(HttpProviders);
    let hotkeys = Arc::new(HotkeyService::new(settings.hotkeys.clone(), tx.clone()));

    app::overlay::configure(app.handle());
    let deps = Deps {
        secrets: secrets.clone(),
        audio: recorder,
        providers: providers.clone(),
        output: Arc::new(SystemOutput::default()),
        overlay: Arc::new(app::overlay::TauriOverlay { app: app.handle().clone() }),
        permissions: Arc::new(SystemPermissions),
        hotkeys: hotkeys.clone(),
        ui: Arc::new(app::services::TauriUi { app: app.handle().clone() }),
        sounds: Arc::new(app::services::TauriSounds { app: app.handle().clone() }),
    };
    let orchestrator = Orchestrator::new(deps, shared.clone(), tx.clone());
    tauri::async_runtime::spawn(orchestrator.run(rx));

    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        while level_rx.changed().await.is_ok() {
            let level = *level_rx.borrow_and_update();
            let _ = handle.emit_to("overlay", "overlay://level", level);
        }
    });

    let needs_setup = secrets.get(settings.provider).is_none() || !permissions::accessibility(false);
    app.manage(AppState { shared, tx, secrets, hotkeys, providers, log_dir });
    if needs_setup {
        show_settings(app.handle());
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings(app);
        }))
        .invoke_handler(tauri::generate_handler![
            app::commands::get_settings,
            app::commands::save_settings,
            app::commands::set_api_key,
            app::commands::api_key_mask,
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
            app::commands::overlay_action,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Utter", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings, &quit])?;

            let mut tray = TrayIconBuilder::with_id("main")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => show_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // Closing the settings window only hides it; the app lives in the tray.
            if let Some(w) = app.get_webview_window("settings") {
                let w2 = w.clone();
                w.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = w2.hide();
                    }
                });
            }
            init_core(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
