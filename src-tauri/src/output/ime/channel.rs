//! Utter ↔ input method channel (spec 2026-11-04 §5.4): a Unix socket in the app data
//! folder (0700, socket 0600), one JSON object per line, one client whose signature is
//! checked before anything is read or sent.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::mpsc;

#[derive(Clone, Debug, PartialEq)]
pub enum ToIm {
    Draft(String),
    Commit(String),
    Clear,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FromIm {
    Hello { version: u32 },
    Active { bundle: String },
    Inactive,
}

pub fn encode(msg: &ToIm) -> String {
    match msg {
        ToIm::Draft(text) => json!({"t": "draft", "text": text}),
        ToIm::Commit(text) => json!({"t": "commit", "text": text}),
        ToIm::Clear => json!({"t": "clear"}),
    }
    .to_string()
}

pub fn decode(line: &str) -> Option<FromIm> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    match v.get("t")?.as_str()? {
        "hello" => Some(FromIm::Hello { version: v.get("version")?.as_u64()? as u32 }),
        "active" => Some(FromIm::Active { bundle: v.get("bundle")?.as_str()?.to_string() }),
        "inactive" => Some(FromIm::Inactive),
        _ => None,
    }
}

/// The field the draft went to, and whether focus left it since.
#[derive(Default)]
struct Watch {
    target: Option<String>,
    moved: bool,
}

#[derive(Default)]
struct State {
    writer: Option<mpsc::UnboundedSender<String>>,
    /// Bumped by every `active`.
    actives: u64,
    last_active: Option<String>,
    watch: Watch,
}

/// The server side, shared by the socket tasks and the dictation.
#[derive(Default)]
pub struct ImChannel {
    state: Mutex<State>,
    changed: Condvar,
}

impl ImChannel {
    /// Applies one message from the input method.
    pub fn handle(&self, msg: FromIm) {
        let Ok(mut s) = self.state.lock() else { return };
        match msg {
            FromIm::Hello { version } => tracing::info!(version, "input method connected"),
            FromIm::Active { bundle } => {
                s.actives += 1;
                match &s.watch.target {
                    Some(t) if *t != bundle => s.watch.moved = true,
                    _ => {}
                }
                s.last_active = Some(bundle);
            }
            FromIm::Inactive => {
                if s.watch.target.is_some() {
                    s.watch.moved = true;
                }
                s.last_active = None;
            }
        }
        drop(s);
        self.changed.notify_all();
    }

    pub fn send(&self, msg: &ToIm) -> bool {
        let Ok(s) = self.state.lock() else { return false };
        s.writer.as_ref().is_some_and(|w| w.send(encode(msg)).is_ok())
    }

    pub fn connected(&self) -> bool {
        self.state.lock().map(|s| s.writer.as_ref().is_some_and(|w| !w.is_closed())).unwrap_or(false)
    }

    /// Bookmark taken before switching to the input method.
    pub fn actives(&self) -> u64 {
        self.state.lock().map(|s| s.actives).unwrap_or(0)
    }

    /// Waits (blocking) for an `active` newer than `since`; that field becomes the target.
    pub fn wait_active(&self, since: u64, timeout: Duration) -> Option<String> {
        let deadline = Instant::now() + timeout;
        let mut s = self.state.lock().ok()?;
        loop {
            if s.actives > since {
                if let Some(b) = s.last_active.clone() {
                    s.watch = Watch { target: Some(b.clone()), moved: false };
                    return Some(b);
                }
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            s = self.changed.wait_timeout(s, left).ok()?.0;
        }
    }

    /// Focus left the target field (an `inactive`, or `active` in another app).
    pub fn focus_moved(&self) -> bool {
        self.state.lock().map(|s| s.watch.target.is_none() || s.watch.moved).unwrap_or(true)
    }

    pub fn reset_watch(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.watch = Watch::default();
        }
    }

    fn attach(&self, writer: mpsc::UnboundedSender<String>) {
        if let Ok(mut s) = self.state.lock() {
            s.writer = Some(writer);
        }
    }
}

pub fn socket_path(data_dir: &Path) -> PathBuf {
    data_dir.join("im.sock")
}

/// Binds the socket (folder 0700, socket 0600) and serves one client at a time; a newer
/// client replaces the older. `peer_ok` checks the client's signature by its socket.
pub async fn serve(
    data_dir: PathBuf,
    channel: Arc<ImChannel>,
    peer_ok: Arc<dyn Fn(i32) -> bool + Send + Sync>,
) -> std::io::Result<()> {
    std::fs::create_dir_all(&data_dir)?;
    std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700))?;
    let path = socket_path(&data_dir);
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    tracing::info!("input method channel listening");
    loop {
        let (stream, _) = listener.accept().await?;
        use std::os::fd::AsRawFd;
        if !peer_ok(stream.as_raw_fd()) {
            tracing::warn!("input method peer rejected");
            continue;
        }
        let (read, mut write) = stream.into_split();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        channel.attach(tx);
        tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                if write.write_all(format!("{line}\n").as_bytes()).await.is_err() {
                    break;
                }
            }
        });
        let ch = channel.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(read).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(msg) = decode(&line) {
                    ch.handle(msg);
                }
            }
            tracing::info!("input method disconnected");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        assert_eq!(encode(&ToIm::Draft("при\"вет".into())), r#"{"t":"draft","text":"при\"вет"}"#);
        assert_eq!(encode(&ToIm::Commit("Привет.".into())), r#"{"t":"commit","text":"Привет."}"#);
        assert_eq!(encode(&ToIm::Clear), r#"{"t":"clear"}"#);
        assert_eq!(decode(r#"{"t":"hello","version":1}"#), Some(FromIm::Hello { version: 1 }));
        assert_eq!(decode(r#"{"bundle":"notion.id","t":"active"}"#), Some(FromIm::Active { bundle: "notion.id".into() }));
        assert_eq!(decode("{\"t\":\"inactive\"}\n"), Some(FromIm::Inactive));
    }

    #[test]
    fn decode_rejects_garbage() {
        for line in ["", "garbage", "{}", r#"{"t":"draft","text":"x"}"#, r#"{"t":"active"}"#, r#"{"t":"hello","version":"x"}"#] {
            assert_eq!(decode(line), None, "{line}");
        }
    }

    #[test]
    fn commit_fails_after_focus_moved() {
        let ch = ImChannel::default();
        let since = ch.actives();
        ch.handle(FromIm::Active { bundle: "notion.id".into() });
        assert_eq!(ch.wait_active(since, Duration::from_millis(10)).as_deref(), Some("notion.id"));
        assert!(!ch.focus_moved());
        ch.handle(FromIm::Active { bundle: "notion.id".into() });
        assert!(!ch.focus_moved(), "the same field again is not a move");
        ch.handle(FromIm::Active { bundle: "com.tdesktop.Telegram".into() });
        assert!(ch.focus_moved());
    }

    #[test]
    fn inactive_is_a_move_and_wait_times_out() {
        let ch = ImChannel::default();
        let since = ch.actives();
        assert_eq!(ch.wait_active(since, Duration::from_millis(20)), None);
        assert!(ch.focus_moved(), "no target yet counts as moved");
        ch.handle(FromIm::Active { bundle: "a".into() });
        ch.wait_active(since, Duration::from_millis(10));
        ch.handle(FromIm::Inactive);
        assert!(ch.focus_moved());
    }

    #[test]
    fn wait_active_wakes_on_message() {
        let ch = Arc::new(ImChannel::default());
        let since = ch.actives();
        let c = ch.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            c.handle(FromIm::Active { bundle: "b".into() });
        });
        let started = Instant::now();
        assert_eq!(ch.wait_active(since, Duration::from_millis(500)).as_deref(), Some("b"));
        assert!(started.elapsed() < Duration::from_millis(300));
        t.join().unwrap();
    }

    #[tokio::test]
    async fn socket_permissions() {
        use tokio::io::AsyncBufReadExt;
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("app.utter.desktop");
        let ch = Arc::new(ImChannel::default());
        let task = tokio::spawn(serve(data.clone(), ch.clone(), Arc::new(|_| true)));
        let path = socket_path(&data);
        for _ in 0..100 {
            if path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&data), 0o700);
        assert_eq!(mode(&path), 0o600);
        // A client talks both ways.
        let client = tokio::net::UnixStream::connect(&path).await.unwrap();
        let (r, mut w) = client.into_split();
        w.write_all(b"{\"t\":\"active\",\"bundle\":\"x\"}\n").await.unwrap();
        for _ in 0..100 {
            if ch.actives() == 1 && ch.connected() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(ch.actives(), 1);
        assert!(ch.send(&ToIm::Clear));
        let mut lines = BufReader::new(r).lines();
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some(r#"{"t":"clear"}"#));
        task.abort();
    }

    #[tokio::test]
    async fn rejected_peer_gets_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let ch = Arc::new(ImChannel::default());
        let task = tokio::spawn(serve(dir.path().to_path_buf(), ch.clone(), Arc::new(|_| false)));
        let path = socket_path(dir.path());
        for _ in 0..100 {
            if path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let mut client = tokio::net::UnixStream::connect(&path).await.unwrap();
        client.write_all(b"{\"t\":\"active\",\"bundle\":\"x\"}\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(ch.actives(), 0);
        assert!(!ch.send(&ToIm::Clear));
        task.abort();
    }
}
