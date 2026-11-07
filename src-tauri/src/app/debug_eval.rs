//! Development builds only (feature `debug-eval`, never in releases): runs
//! `<app data>/debug-eval.js` in the settings page once, then deletes it. Lets the
//! developer drive and inspect the real window (clicks, state) without the user's hands.

use std::path::PathBuf;
use std::time::Duration;

use tauri::{AppHandle, Manager};

pub fn start(app: AppHandle, data_dir: PathBuf) {
    let path = data_dir.join("debug-eval.js");
    let draft = data_dir.join("debug-draft.txt");
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(300));
        if let Ok(text) = std::fs::read_to_string(&draft) {
            let _ = std::fs::remove_file(&draft);
            let a = app.clone();
            std::thread::spawn(move || draft_demo(&a, text.trim()));
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

/// `<app data>/debug-draft.txt` with a phrase: the island records, opens the draft window as the
/// words arrive (every 150 ms), processes for 0.5 s, then shows "done" — to check the window and
/// its exit on screen without a dictation.
fn draft_demo(app: &AppHandle, text: &str) {
    use super::orchestrator::{OverlaySink, OverlayState};
    use crate::live::draft::DraftParts;
    use crate::settings::TranslateStyle;
    use crate::types::Mode;
    let Some(presence) = app.try_state::<super::overlay_rest::RestPresence>().map(|p| p.0.clone()) else { return };
    let overlay = super::overlay::TauriOverlay { app: app.clone(), gate: Default::default(), presence };
    let now = chrono::Local::now().timestamp_millis();
    overlay.show(&OverlayState::Recording { mode: Mode::Dictation, style: TranslateStyle::Business, started_at_ms: now, warn: false, pending: 0 });
    std::thread::sleep(Duration::from_millis(900));
    let words: Vec<&str> = text.split_whitespace().collect();
    for n in 1..=words.len() {
        let settled = n.saturating_sub(3);
        overlay.draft(Some(&DraftParts { settled: words[..settled].join(" "), interim: words[settled..n].join(" ") }));
        std::thread::sleep(Duration::from_millis(150));
    }
    overlay.show(&OverlayState::Processing { pending: 0, since_ms: chrono::Local::now().timestamp_millis() });
    std::thread::sleep(Duration::from_millis(500));
    overlay.draft(None);
    overlay.show(&OverlayState::Done { outcome: "pasted".into(), degraded: false });
    std::thread::sleep(Duration::from_millis(2_200));
    overlay.show(&OverlayState::Hidden);
    tracing::info!("draft demo: end");
}
