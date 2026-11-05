//! The live draft in other apps' fields through Utter's input method (spec 2026-11-04 §5.3–5.5):
//! switch the layout to "Utter", wake the field with F19, send draft/commit over the channel,
//! and always switch the user's layout back.

pub mod channel;
#[cfg(target_os = "macos")]
pub mod install;
#[cfg(target_os = "macos")]
pub mod peer;
#[cfg(target_os = "macos")]
pub mod tis;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use channel::{ImChannel, ToIm};

/// Not `app.utter.desktop.inputmethod`: macOS kept mapping that path to the 2026-11-04
/// prototype and would not enable it (2026-11-05).
pub const IM_ID: &str = "app.utter.inputmethod.live";
/// F19: the input method swallows it; posting it makes the field connect at once.
pub const WAKE_KEY: u16 = 80;
/// How long the field may take to connect after the wake key.
pub const CONNECT_WAIT: Duration = Duration::from_millis(300);
/// The user's layout during a dictation, so a crash can be undone at the next start.
pub const SAVED_SOURCE_FILE: &str = "input-source";

pub trait Tis: Send + Sync {
    fn current_id(&self) -> Option<String>;
    fn select(&self, id: &str) -> bool;
}

/// A field that shows the draft while speaking. `end` is called after every dictation.
pub trait DraftField: Send + Sync {
    /// Switches to the input method; `true` once the focused field connected.
    fn begin(&self) -> bool;
    fn draft(&self, text: &str);
    /// Replaces the draft with `text`; `false` when focus moved since `begin` (nothing sent).
    fn commit(&self, text: &str) -> bool;
    fn clear(&self);
    /// Switches the user's layout back.
    fn end(&self);
    fn focus_changed(&self) -> bool;
}

/// Where the input method is not available: no draft, the usual delivery.
pub struct NoDraftField;

impl DraftField for NoDraftField {
    fn begin(&self) -> bool {
        false
    }
    fn draft(&self, _: &str) {}
    fn commit(&self, _: &str) -> bool {
        false
    }
    fn clear(&self) {}
    fn end(&self) {}
    fn focus_changed(&self) -> bool {
        true
    }
}

/// What the draft field needs from the channel (a fake in tests).
pub trait ImLink: Send + Sync {
    fn actives(&self) -> u64;
    fn wait_active(&self, since: u64, timeout: Duration) -> Option<String>;
    fn send(&self, msg: &ToIm) -> bool;
    fn focus_moved(&self) -> bool;
    fn reset_watch(&self);
}

impl ImLink for ImChannel {
    fn actives(&self) -> u64 {
        ImChannel::actives(self)
    }
    fn wait_active(&self, since: u64, timeout: Duration) -> Option<String> {
        ImChannel::wait_active(self, since, timeout)
    }
    fn send(&self, msg: &ToIm) -> bool {
        ImChannel::send(self, msg)
    }
    fn focus_moved(&self) -> bool {
        ImChannel::focus_moved(self)
    }
    fn reset_watch(&self) {
        ImChannel::reset_watch(self)
    }
}

pub struct ImeDraftField {
    tis: Arc<dyn Tis>,
    link: Arc<dyn ImLink>,
    data_dir: PathBuf,
    wake: Arc<dyn Fn() + Send + Sync>,
    saved: Mutex<Option<String>>,
}

impl ImeDraftField {
    pub fn new(tis: Arc<dyn Tis>, link: Arc<dyn ImLink>, data_dir: PathBuf, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self { tis, link, data_dir, wake, saved: Mutex::new(None) }
    }

    fn saved_file(&self) -> PathBuf {
        self.data_dir.join(SAVED_SOURCE_FILE)
    }
}

impl DraftField for ImeDraftField {
    fn begin(&self) -> bool {
        self.link.reset_watch();
        // A leftover file means the last dictation never ended: its layout is the user's.
        let user = match self.tis.current_id() {
            Some(id) if id != IM_ID => Some(id),
            _ => std::fs::read_to_string(self.saved_file()).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        };
        let Some(user) = user else { return false };
        let _ = std::fs::write(self.saved_file(), &user);
        if let Ok(mut s) = self.saved.lock() {
            *s = Some(user);
        }
        let since = self.link.actives();
        if !self.tis.select(IM_ID) {
            return false;
        }
        (self.wake)();
        let connected = self.link.wait_active(since, CONNECT_WAIT);
        tracing::info!(connected = connected.is_some(), field = connected.as_deref().unwrap_or(""), "live draft field");
        connected.is_some()
    }

    fn draft(&self, text: &str) {
        self.link.send(&ToIm::Draft(text.to_string()));
    }

    fn commit(&self, text: &str) -> bool {
        if self.link.focus_moved() {
            return false;
        }
        self.link.send(&ToIm::Commit(text.to_string()))
    }

    fn clear(&self) {
        self.link.send(&ToIm::Clear);
    }

    fn end(&self) {
        let saved = self.saved.lock().ok().and_then(|mut s| s.take());
        if let Some(user) = saved {
            // The user may have picked a layout by hand meanwhile; only undo our own switch.
            let current = self.tis.current_id();
            if current.is_none() || current.as_deref() == Some(IM_ID) {
                self.tis.select(&user);
            }
        }
        let _ = std::fs::remove_file(self.saved_file());
        self.link.reset_watch();
    }

    fn focus_changed(&self) -> bool {
        self.link.focus_moved()
    }
}

/// At Utter's start: a dictation cut short by a crash leaves the layout on "Utter"; switch back.
pub fn restore_after_crash(data_dir: &Path, tis: &dyn Tis) {
    let file = data_dir.join(SAVED_SOURCE_FILE);
    let Ok(saved) = std::fs::read_to_string(&file) else { return };
    let saved = saved.trim();
    if !saved.is_empty() && tis.current_id().as_deref() == Some(IM_ID) {
        let ok = tis.select(saved);
        tracing::info!(ok, "input source restored after an unfinished dictation");
    }
    let _ = std::fs::remove_file(file);
}

pub struct DraftFieldState(pub Arc<dyn DraftField>);

/// At Utter's start: undo a crashed dictation's layout, serve the channel, install the input
/// method and start its process (so the first dictation's field connects at once).
#[cfg(target_os = "macos")]
pub fn start(bundled: PathBuf, data_dir: PathBuf) -> Arc<dyn DraftField> {
    restore_after_crash(&data_dir, &tis::SystemTis);
    let channel = Arc::new(ImChannel::default());
    let peer_ok: Arc<dyn Fn(i32) -> bool + Send + Sync> = match peer::own_leaf_sha1() {
        Some(leaf) => {
            let req = peer::requirement(peer::IM_IDENTIFIER, &leaf);
            Arc::new(move |fd| peer::check_peer(fd, &req))
        }
        None => {
            tracing::warn!("Utter is not signed: the live draft is off");
            Arc::new(|_| false)
        }
    };
    let (ch, dir) = (channel.clone(), data_dir.clone());
    tauri::async_runtime::spawn(async move {
        if let Err(e) = channel::serve(dir, ch, peer_ok).await {
            tracing::warn!("input method channel stopped: {e}");
        }
    });
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {
        let _ = crate::output::macos::MacInput::tap_key(WAKE_KEY);
    });
    let (w, ch) = (wake.clone(), channel.clone());
    std::thread::spawn(move || {
        match install::install(&bundled) {
            Ok(copied) => tracing::info!(copied, "input method installed"),
            Err(e) => return tracing::warn!("input method not installed: {e}"),
        }
        if !tis::is_enabled(IM_ID) {
            return tracing::warn!("input method not enabled: the user adds «Utter» in System Settings → Keyboard");
        }
        std::thread::sleep(Duration::from_millis(1_500));
        if !ch.connected() {
            install::warm_up(&tis::SystemTis, &*w);
            tracing::info!("input method warmed up");
        }
    });
    Arc::new(ImeDraftField::new(Arc::new(tis::SystemTis), channel, data_dir, wake))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeTis {
        current: Mutex<Option<String>>,
        selected: Mutex<Vec<String>>,
    }
    impl FakeTis {
        fn on(id: &str) -> Arc<Self> {
            let t = Self::default();
            *t.current.lock().unwrap() = Some(id.into());
            Arc::new(t)
        }
        fn current(&self) -> Option<String> {
            self.current.lock().unwrap().clone()
        }
    }
    impl Tis for FakeTis {
        fn current_id(&self) -> Option<String> {
            self.current()
        }
        fn select(&self, id: &str) -> bool {
            *self.current.lock().unwrap() = Some(id.into());
            self.selected.lock().unwrap().push(id.into());
            true
        }
    }

    /// A channel that never connects (no input method running).
    struct DeadLink;
    impl ImLink for DeadLink {
        fn actives(&self) -> u64 {
            0
        }
        fn wait_active(&self, _: u64, _: Duration) -> Option<String> {
            None
        }
        fn send(&self, _: &ToIm) -> bool {
            false
        }
        fn focus_moved(&self) -> bool {
            true
        }
        fn reset_watch(&self) {}
    }

    const USER: &str = "com.apple.keylayout.RussianWin";

    #[test]
    fn startup_restores_saved_layout() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(SAVED_SOURCE_FILE), USER).unwrap();
        let tis = FakeTis::on(IM_ID);
        restore_after_crash(d.path(), tis.as_ref());
        assert_eq!(tis.current().as_deref(), Some(USER));
        assert!(!d.path().join(SAVED_SOURCE_FILE).exists());
        // No file: nothing changes.
        let other = FakeTis::on("com.apple.keylayout.ABC");
        restore_after_crash(d.path(), other.as_ref());
        assert!(other.selected.lock().unwrap().is_empty());
    }

    #[test]
    fn end_always_restores() {
        let d = tempfile::tempdir().unwrap();
        let tis = FakeTis::on(USER);
        let field = ImeDraftField::new(tis.clone(), Arc::new(DeadLink), d.path().to_path_buf(), Arc::new(|| {}));
        assert!(!field.begin(), "the field never connected");
        assert_eq!(tis.current().as_deref(), Some(IM_ID));
        assert_eq!(std::fs::read_to_string(d.path().join(SAVED_SOURCE_FILE)).unwrap(), USER);
        assert!(!field.commit("текст"));
        field.end();
        assert_eq!(tis.current().as_deref(), Some(USER));
        assert!(!d.path().join(SAVED_SOURCE_FILE).exists());
    }

    #[test]
    fn begin_connects_and_commits_through_the_channel() {
        let d = tempfile::tempdir().unwrap();
        let tis = FakeTis::on(USER);
        let ch = Arc::new(ImChannel::default());
        let c = ch.clone();
        // The input method reports the field once the wake key is posted.
        let wake = Arc::new(move || c.handle(channel::FromIm::Active { bundle: "notion.id".into() }));
        let field = ImeDraftField::new(tis.clone(), ch.clone(), d.path().to_path_buf(), wake);
        assert!(field.begin());
        assert!(!field.focus_changed());
        ch.handle(channel::FromIm::Inactive);
        assert!(field.focus_changed());
        assert!(!field.commit("Текст."), "focus moved: nothing is committed");
        field.end();
        assert_eq!(tis.current().as_deref(), Some(USER));
    }

    #[test]
    fn end_leaves_a_layout_the_user_picked() {
        let d = tempfile::tempdir().unwrap();
        let tis = FakeTis::on(USER);
        let field = ImeDraftField::new(tis.clone(), Arc::new(DeadLink), d.path().to_path_buf(), Arc::new(|| {}));
        field.begin();
        tis.select("com.apple.keylayout.ABC");
        field.end();
        assert_eq!(tis.current().as_deref(), Some("com.apple.keylayout.ABC"));
    }

    #[test]
    fn leftover_file_names_the_users_layout() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(SAVED_SOURCE_FILE), USER).unwrap();
        let tis = FakeTis::on(IM_ID);
        let field = ImeDraftField::new(tis.clone(), Arc::new(DeadLink), d.path().to_path_buf(), Arc::new(|| {}));
        field.begin();
        field.end();
        assert_eq!(tis.current().as_deref(), Some(USER));
    }
}
