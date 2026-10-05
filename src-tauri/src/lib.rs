pub mod audio;
pub mod controller;
pub mod hotkey;
pub mod jobs;
pub mod output;
pub mod pipeline;
pub mod providers;
pub mod secrets;
pub mod settings;
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
    let outcome = match output::clipboard::ArboardClipboard::new() {
        Ok(mut cb) => output::deliver(&text, method, &mut cb, &output::macos::MacInput),
        Err(e) => return Some(format!("clipboard unavailable: {e}")),
    };
    Some(format!("{outcome:?}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings(app);
        }))
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
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
