//! Development builds only (feature `debug-eval`, never in releases): runs
//! `<app data>/debug-eval.js` in the settings page once, then deletes it. Lets the
//! developer drive and inspect the real window (clicks, state) without the user's hands.

use std::path::PathBuf;
use std::time::Duration;

use tauri::{AppHandle, Manager};

pub fn start(app: AppHandle, data_dir: PathBuf) {
    let path = data_dir.join("debug-eval.js");
    let im = data_dir.join("debug-im.txt");
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(300));
        if let Ok(text) = std::fs::read_to_string(&im) {
            let _ = std::fs::remove_file(&im);
            let field = app.state::<crate::output::ime::DraftFieldState>().0.clone();
            std::thread::spawn(move || im_demo(field.as_ref(), text.trim()));
        }
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

/// `<app data>/debug-im.txt` with a phrase: shows it word by word as the draft in the focused
/// field, then commits it, then switches the layout back. Logs each step.
fn im_demo(field: &dyn crate::output::ime::DraftField, text: &str) {
    let t = std::time::Instant::now();
    let connected = field.begin();
    tracing::info!(connected, ms = t.elapsed().as_millis() as u64, "im demo: begin");
    if connected {
        let words: Vec<&str> = text.split_whitespace().collect();
        for n in 1..=words.len() {
            field.draft(&words[..n].join(" "));
            std::thread::sleep(Duration::from_millis(400));
        }
        std::thread::sleep(Duration::from_millis(800));
        let committed = field.commit(text);
        tracing::info!(committed, focus_changed = field.focus_changed(), "im demo: commit");
    }
    field.end();
    tracing::info!(ms = t.elapsed().as_millis() as u64, "im demo: end");
}
