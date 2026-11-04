//! Auto-update through GitHub Releases (§3.7): checked at start and every 24 h, installed
//! only when the user presses "Install".

use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// What the About section and the tray show.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available { version: String, notes: Option<String> },
    Downloading { percent: u8 },
    Ready,
    Failed,
}

const DAY: Duration = Duration::from_secs(24 * 3600);
/// The first check waits until the app has settled after start.
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(10);
/// How often the background task asks whether a check is due. Wall time, so a night of sleep counts.
const TICK: Duration = Duration::from_secs(3600);

/// A check is due if there was none yet, a day has passed, or the clock went backwards.
pub fn due(last: Option<SystemTime>, now: SystemTime) -> bool {
    match last {
        None => true,
        Some(last) => now.duration_since(last).map(|d| d >= DAY).unwrap_or(true),
    }
}

/// Download progress, 0..=100; 0 while the size is unknown.
pub fn percent(downloaded: u64, total: Option<u64>) -> u8 {
    match total {
        Some(t) if t > 0 => (downloaded.saturating_mul(100) / t).min(100) as u8,
        _ => 0,
    }
}

/// A new check may start unless one is running or an update is being installed.
pub fn can_check(s: &UpdateState) -> bool {
    !matches!(s, UpdateState::Checking | UpdateState::Downloading { .. } | UpdateState::Ready)
}

/// The version the tray offers to update to, if any.
pub fn offered_version(s: &UpdateState) -> Option<&str> {
    match s {
        UpdateState::Available { version, .. } => Some(version),
        _ => None,
    }
}

/// The update state shared by the commands and the background task.
pub struct Updates {
    state: Mutex<UpdateState>,
    last: Mutex<Option<SystemTime>>,
    found: Mutex<Option<Update>>,
}

impl Default for Updates {
    fn default() -> Self {
        Self { state: Mutex::new(UpdateState::Idle), last: Mutex::new(None), found: Mutex::new(None) }
    }
}

impl Updates {
    pub fn state(&self) -> UpdateState {
        self.state.lock().map(|s| s.clone()).unwrap_or(UpdateState::Idle)
    }
}

fn publish(app: &AppHandle, s: UpdateState) {
    let Some(u) = app.try_state::<Updates>() else { return };
    if let Ok(mut st) = u.state.lock() {
        *st = s.clone();
    }
    super::tray::set_update(app, offered_version(&s));
    let _ = app.emit("ui://update", &s);
}

/// Asks GitHub Releases for a newer version. Never installs by itself.
pub async fn check(app: AppHandle) {
    let Some(u) = app.try_state::<Updates>() else { return };
    let start = u.state.lock().map(|mut st| {
        let ok = can_check(&st);
        if ok {
            *st = UpdateState::Checking;
        }
        ok
    });
    if !start.unwrap_or(false) {
        return;
    }
    publish(&app, UpdateState::Checking);
    if let Ok(mut last) = u.last.lock() {
        *last = Some(SystemTime::now());
    }
    let result = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    let next = match result {
        Ok(Some(update)) => {
            let next = UpdateState::Available { version: update.version.clone(), notes: update.body.clone() };
            if let Ok(mut f) = u.found.lock() {
                *f = Some(update);
            }
            next
        }
        Ok(None) => UpdateState::UpToDate,
        Err(e) => {
            // While the repository is private (until release) this is the normal answer.
            tracing::warn!("update check failed: {e}");
            UpdateState::Failed
        }
    };
    tracing::info!(?next, "update check");
    publish(&app, next);
}

/// Downloads and installs the found update with progress, then restarts Utter.
pub async fn install(app: AppHandle) {
    let Some(u) = app.try_state::<Updates>() else { return };
    let Some(update) = u.found.lock().ok().and_then(|mut f| f.take()) else { return };
    publish(&app, UpdateState::Downloading { percent: 0 });
    let mut got: u64 = 0;
    let mut shown = 0u8;
    let progress_app = app.clone();
    let result = update
        .download_and_install(
            |chunk, total| {
                got += chunk as u64;
                let p = percent(got, total);
                if p != shown {
                    shown = p;
                    publish(&progress_app, UpdateState::Downloading { percent: p });
                }
            },
            || {},
        )
        .await;
    match result {
        Ok(()) => {
            publish(&app, UpdateState::Ready);
            tracing::info!(version = %update.version, "update installed, restarting");
            app.restart();
        }
        Err(e) => {
            tracing::warn!("update install failed: {e}");
            publish(&app, UpdateState::Failed);
        }
    }
}

/// First check shortly after start, then whenever a day has passed (checked hourly).
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_AFTER).await;
        loop {
            let last = app.try_state::<Updates>().and_then(|u| u.last.lock().ok().map(|l| *l)).flatten();
            if due(last, SystemTime::now()) {
                check(app.clone()).await;
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    const H: Duration = Duration::from_secs(3600);

    #[test]
    fn first_check_is_due() {
        assert!(due(None, SystemTime::now()));
    }

    #[test]
    fn check_is_due_after_24_hours() {
        let last = SystemTime::UNIX_EPOCH + 1000 * H;
        assert!(!due(Some(last), last + 23 * H));
        assert!(due(Some(last), last + 24 * H));
    }

    #[test]
    fn clock_going_backwards_is_due() {
        let last = SystemTime::UNIX_EPOCH + 1000 * H;
        assert!(due(Some(last), last - H));
    }

    #[test]
    fn download_percent() {
        assert_eq!(percent(0, Some(200)), 0);
        assert_eq!(percent(50, Some(200)), 25);
        assert_eq!(percent(300, Some(200)), 100);
        assert_eq!(percent(50, None), 0);
        assert_eq!(percent(50, Some(0)), 0);
    }

    #[test]
    fn only_one_check_or_install_at_a_time() {
        assert!(can_check(&UpdateState::Idle));
        assert!(can_check(&UpdateState::UpToDate));
        assert!(can_check(&UpdateState::Failed));
        assert!(!can_check(&UpdateState::Checking));
        assert!(!can_check(&UpdateState::Downloading { percent: 10 }));
        assert!(!can_check(&UpdateState::Ready));
        // A found update stays offered until installed or a later check says otherwise.
        assert!(can_check(&UpdateState::Available { version: "1.2.0".into(), notes: None }));
    }

    #[test]
    fn tray_offers_the_new_version() {
        assert_eq!(offered_version(&UpdateState::Available { version: "1.2.0".into(), notes: None }), Some("1.2.0"));
        assert_eq!(offered_version(&UpdateState::Failed), None);
        assert_eq!(offered_version(&UpdateState::UpToDate), None);
    }

    #[test]
    fn state_reaches_the_page_tagged() {
        let v = serde_json::to_value(UpdateState::Downloading { percent: 42 }).unwrap();
        assert_eq!(v, serde_json::json!({ "kind": "downloading", "percent": 42 }));
        let v = serde_json::to_value(UpdateState::UpToDate).unwrap();
        assert_eq!(v, serde_json::json!({ "kind": "up_to_date" }));
    }
}
