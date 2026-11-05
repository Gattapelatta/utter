//! Development builds only (feature `debug-eval`, never in releases): runs
//! `<app data>/debug-eval.js` in the settings page once, then deletes it. Lets the
//! developer drive and inspect the real window (clicks, state) without the user's hands.

use std::path::PathBuf;
use std::time::Duration;

use tauri::{AppHandle, Manager};

pub fn start(app: AppHandle, data_dir: PathBuf) {
    let path = data_dir.join("debug-eval.js");
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(300));
        let Ok(js) = std::fs::read_to_string(&path) else { continue };
        let _ = std::fs::remove_file(&path);
        let a = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(w) = a.get_webview_window(super::settings_window::LABEL) {
                let r = w.eval(&js);
                tracing::info!(?r, "debug eval");
            }
        });
    });
}

