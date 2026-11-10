//! The agents service inside Utter: takes events from `utter-agent` over the socket, keeps the
//! registry, sends `overlay://agents` to the island, connects and disconnects the agents.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

pub use super::broker::Reply;
use super::broker::Broker;
use super::event::{parse_claude, parse_codex, EventKind, SessionKey};
use super::install::{self, Detected};
use super::registry::{AgentView, Card, Registry};
use super::wire::{Request, Source};
use crate::app::overlay_rest::Rect;
use crate::app::tray::Lang;
use crate::settings::{AgentSettings, Settings};

/// Waiting for the provider's summary line.
const SUMMARY_TIMEOUT: Duration = Duration::from_secs(3);
/// The longest line `utter-agent` may send (a huge last message is still well below this).
const LINE_MAX: u64 = 4 << 20;

// ---------- Connecting the agents ----------

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum Link {
    Off,
    On,
    /// Switched on, but the agent is not on this Mac.
    Missing,
    /// Switched on, but its settings file could not be used (and was left alone).
    Broken(String),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AgentsStatus {
    pub detected: Detected,
    pub claude_code: Link,
    pub codex: Link,
}

const MARK: &str = "utter-agent";

fn claude_file(home: &Path) -> PathBuf {
    home.join(".claude/settings.json")
}

fn codex_file(home: &Path) -> PathBuf {
    home.join(".codex/config.toml")
}

/// `Ok(None)`: no file.
fn read(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Writes through a symlink (dotfile managers keep these files as links), atomically, keeping
/// the file's permissions: these files may hold tokens (review 2026-11-10).
fn write(path: &Path, text: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mode = std::fs::metadata(&real).ok().map(|m| m.permissions().mode() & 0o7777);
    crate::settings::write_atomic_mode(&real, text.as_bytes(), mode).map_err(|e| e.to_string())
}

fn claude_link(home: &Path, helper: &str, on: bool, found: bool) -> Link {
    if !on {
        return Link::Off;
    }
    if !found {
        return Link::Missing;
    }
    match read(&claude_file(home)) {
        Err(e) => Link::Broken(e),
        Ok(text) => {
            let text = text.unwrap_or_default();
            if install::claude_installed(&text, helper) {
                Link::On
            } else {
                match install::claude_install(Some(&text), helper) {
                    Err(install::InstallError::Corrupt(e)) => Link::Broken(e),
                    Ok(_) => Link::Broken("not connected".into()),
                }
            }
        }
    }
}

fn codex_link(home: &Path, helper: &str, on: bool, found: bool) -> Link {
    if !on {
        return Link::Off;
    }
    if !found {
        return Link::Missing;
    }
    match read(&codex_file(home)) {
        Err(e) => Link::Broken(e),
        Ok(text) => {
            let text = text.unwrap_or_default();
            if install::codex_installed(&text, helper) {
                Link::On
            } else {
                match install::codex_install(Some(&text), helper) {
                    Err(install::InstallError::Corrupt(e)) => Link::Broken(e),
                    Ok(_) => Link::Broken("not connected".into()),
                }
            }
        }
    }
}

/// Where each agent stands, without changing anything.
pub fn status_in(home: &Path, helper: &str, s: &AgentSettings, detected: Detected) -> AgentsStatus {
    AgentsStatus {
        detected,
        claude_code: claude_link(home, helper, s.claude_code, detected.claude_code),
        codex: codex_link(home, helper, s.codex, detected.codex),
    }
}

/// Makes the agents' files match the settings: connects what is on (or fixes a moved path),
/// takes Utter out of what is off. Writes only when something changes. Returns the status and
/// Codex's previous `notify` to keep in the settings.
pub fn apply_in(
    home: &Path,
    helper: &str,
    s: &AgentSettings,
    prev: Option<Vec<String>>,
    detected: Detected,
) -> (AgentsStatus, Option<Vec<String>>) {
    let claude = claude_file(home);
    if let Ok(text) = read(&claude) {
        if s.claude_code && detected.claude_code {
            let current = text.unwrap_or_default();
            if !install::claude_installed(&current, helper) {
                if let Ok(out) = install::claude_install(Some(&current), helper) {
                    let _ = write(&claude, &out).map_err(|e| tracing::warn!("claude settings not written: {e}"));
                }
            }
        } else if let Some(current) = text.filter(|t| t.contains(MARK)) {
            if let Ok(out) = install::claude_uninstall(&current) {
                let _ = write(&claude, &out).map_err(|e| tracing::warn!("claude settings not written: {e}"));
            }
        }
    }
    let codex = codex_file(home);
    let mut kept = None;
    if let Ok(text) = read(&codex) {
        if s.codex && detected.codex {
            let current = text.unwrap_or_default();
            if install::codex_installed(&current, helper) {
                kept = prev;
            } else if let Ok((out, p)) = install::codex_install(Some(&current), helper) {
                match write(&codex, &out) {
                    Ok(()) => kept = p,
                    Err(e) => tracing::warn!("codex config not written: {e}"),
                }
            }
        } else if let Some(current) = text.filter(|t| t.contains(MARK)) {
            let out = install::codex_uninstall(&current, prev.as_deref());
            let _ = write(&codex, &out).map_err(|e| tracing::warn!("codex config not written: {e}"));
        }
    }
    (status_in(home, helper, s, detected), kept)
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

/// A path the hooks may point at: not the disk image and not macOS's temporary copy of an app
/// opened from Downloads (AppTranslocation) — both vanish (review 2026-11-10).
pub fn stable_helper(path: &str) -> bool {
    !path.starts_with("/Volumes/") && !path.contains("/AppTranslocation/")
}

/// `utter-agent` next to Utter's own binary, if the hooks may point at it.
fn helper_path() -> Option<String> {
    let path = std::env::current_exe().ok()?.parent()?.join("utter-agent");
    let path = path.exists().then(|| path.to_string_lossy().into_owned())?;
    if !stable_helper(&path) {
        tracing::warn!(path, "Utter runs from a temporary place: agents not connected");
        return None;
    }
    Some(path)
}

/// Applies the agents part of the settings to the agents' files; returns the settings with
/// Codex's previous `notify` up to date. Called at startup and before every settings save.
pub fn reconcile(mut next: Settings) -> Settings {
    let Some(helper) = helper_path() else {
        tracing::warn!("utter-agent not found next to Utter: agents not connected");
        return next;
    };
    let home = home_dir();
    let (status, prev) = apply_in(&home, &helper, &next.agents, next.agents_codex_prev_notify.take(), install::detect(&home));
    tracing::info!(?status, "agents connected");
    next.agents_codex_prev_notify = prev;
    next
}

pub fn status(s: &AgentSettings) -> AgentsStatus {
    let home = home_dir();
    let detected = install::detect(&home);
    match helper_path() {
        Some(helper) => status_in(&home, &helper, s, detected),
        None => {
            let broken = |on| if on { Link::Broken("utter-agent not found".into()) } else { Link::Off };
            AgentsStatus { detected, claude_code: broken(s.claude_code), codex: broken(s.codex) }
        }
    }
}

// ---------- Events and permissions ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    Allow,
    Always,
    Deny,
    /// «Открыть»: no decision here, Claude Code asks in its own window.
    Open,
}

pub enum Incoming {
    /// Not an event the Drop shows.
    Ignored,
    /// The view changed.
    Updated,
    /// Answer the helper right away.
    Reply(Reply),
    /// A permission request: answer the helper when `rx` resolves.
    Ask { id: u64, rx: oneshot::Receiver<Reply> },
    /// The agent finished: write the summary line.
    Finished { key: SessionKey, last_message: String },
}

#[derive(Default)]
pub struct Core {
    reg: Mutex<Registry>,
    pub broker: Broker,
    /// The helper's ancestors per session: where «Открыть» looks for the app.
    ancestors: Mutex<HashMap<SessionKey, Vec<i32>>>,
}

impl Core {
    fn reg(&self) -> std::sync::MutexGuard<'_, Registry> {
        self.reg.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn receive(&self, req: Request, lang: Lang, now: Instant) -> Incoming {
        let event = match req.source {
            Source::Claude => parse_claude(&req.event, &req.payload, lang),
            Source::Codex => parse_codex(&req.payload),
        };
        let Some(event) = event else {
            return if req.wants_reply { Incoming::Reply(Reply::NoDecision) } else { Incoming::Ignored };
        };
        if !req.ancestors.is_empty() {
            if let Ok(mut a) = self.ancestors.lock() {
                a.insert(event.key.clone(), req.ancestors.clone());
            }
        }
        let finished = match &event.kind {
            EventKind::Finished { last_message } => Some((event.key.clone(), last_message.clone())),
            _ => None,
        };
        if event.kind == EventKind::Ended {
            // The session's open requests end with it; nobody will click them.
            let open = self.reg().permission_ids(&event.key);
            self.reg().apply(event.clone(), now);
            for id in open {
                self.broker.answer(id, Reply::NoDecision);
            }
            if let Ok(mut a) = self.ancestors.lock() {
                a.remove(&event.key);
            }
            return if req.wants_reply { Incoming::Reply(Reply::NoDecision) } else { Incoming::Updated };
        }
        let id = self.reg().apply(event, now);
        if let Some(id) = id {
            return Incoming::Ask { id, rx: self.broker.open(id) };
        }
        if req.wants_reply {
            return Incoming::Reply(Reply::NoDecision);
        }
        match finished {
            Some((key, last_message)) => Incoming::Finished { key, last_message },
            None => Incoming::Updated,
        }
    }

    /// `idle`: no dictation on screen, so the top card is visible and its time runs.
    pub fn tick(&self, now: Instant, idle: bool) {
        let mut reg = self.reg();
        if idle {
            reg.card_shown(now);
            if let Some(Card::Permission { id, .. }) = reg.view().card {
                self.broker.shown(id, now);
            }
        }
        reg.tick(now);
        for id in self.broker.expire(now) {
            reg.resolve_permission(id);
        }
    }

    pub fn answer(&self, id: u64, choice: Choice) {
        let reply = match choice {
            Choice::Allow => Reply::Allow { always: false },
            Choice::Always => Reply::Allow { always: true },
            Choice::Deny => Reply::Deny,
            Choice::Open => Reply::NoDecision,
        };
        self.broker.answer(id, reply);
        self.reg().resolve_permission(id);
    }

    /// The helper went away (Esc in Claude Code, its own timeout): no answer, no card.
    pub fn abandon(&self, id: u64) {
        self.broker.answer(id, Reply::NoDecision);
        self.reg().resolve_permission(id);
    }

    pub fn set_paused(&self, on: bool, now: Instant) {
        for id in self.reg().set_paused(on, now) {
            self.broker.answer(id, Reply::NoDecision);
        }
    }

    pub fn paused(&self) -> bool {
        self.reg().view().paused
    }

    /// Agents switched off: their sessions, cards and waiting requests go.
    pub fn forget_disabled(&self, s: &AgentSettings) {
        use crate::agents::event::Agent;
        for (agent, on) in [(Agent::ClaudeCode, s.claude_code), (Agent::Codex, s.codex)] {
            if !on {
                for id in self.reg().forget_agent(agent) {
                    self.broker.answer(id, Reply::NoDecision);
                }
            }
        }
    }

    pub fn set_summary(&self, key: &SessionKey, s: Option<String>) {
        self.reg().set_summary(key, s);
    }

    pub fn dismiss(&self, now: Instant) {
        self.reg().dismiss_card(now);
    }

    pub fn seen(&self, key: &SessionKey) {
        self.reg().mark_seen(key);
    }

    pub fn ancestors(&self, key: &SessionKey) -> Option<Vec<i32>> {
        self.ancestors.lock().ok()?.get(key).cloned()
    }

    pub fn view(&self) -> AgentView {
        self.reg().view()
    }
}

/// The view as the island gets it: with whether an agent is switched on (the user 2026-11-10:
/// the right ear keeps its three colours while one is).
pub fn with_connection(mut view: AgentView, s: &AgentSettings) -> AgentView {
    view.connected = s.claude_code || s.codex;
    view
}

// ---------- Pointer moves near the top ----------

/// How far below a display's top edge pointer moves matter (the window's height, with slack).
pub const BAND: f64 = 240.0;

/// A move `from_top` points below the display's top: is it in the band now, and must the
/// watcher look at once (in the band, or just left it)?
pub fn pointer_band(from_top: f64, was_inside: bool) -> (bool, bool) {
    let inside = from_top < BAND;
    (inside, inside || was_inside)
}

// ---------- How often the watcher looks ----------

/// 100 ms while a card is open or the pointer is over the ears (hover must feel instant);
/// 300 ms with only dots (an unseen green dot may stay for hours); 500 ms with bare ears
/// (pokes); 1 s otherwise. Review 2026-11-10: 10 Hz for a dot was a constant cost.
pub fn watch_period(view: &AgentView, hover: bool, ears: bool) -> Duration {
    let ms = if view.card.is_some() || hover {
        100
    } else if !view.rows.is_empty() {
        300
    } else if ears {
        500
    } else {
        1000
    };
    Duration::from_millis(ms)
}

/// Whether the pointer matters at all (otherwise it is not even read).
pub fn needs_pointer(view: &AgentView, ears: bool) -> bool {
    !view.rows.is_empty() || view.card.is_some() || ears
}

// ---------- Where the pointer counts as "on the Drop" ----------

/// Width of the ears together (the notch ≈ 179 pt and two 36 pt ears, with a little slack).
const EARS_W: f64 = 260.0;
/// Height of the ears' strip at the top of the window.
const EARS_H: f64 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Zone {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Zone {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// The strip at the top of the screen (the notch's height, with a little slack) where the window
/// never takes clicks: there a full-screen app shows its menu and title bar, and a window taking
/// the clicks kept them from hiding again (the user 2026-11-10).
pub const TOP_STRIP: f64 = 36.0;

/// The window takes clicks only right over something to click — `zones`, in the window's own
/// points, as the island page reports them — and never in the top strip.
pub fn takes_clicks(win: Rect, cursor: (f64, f64), zones: &[Zone]) -> bool {
    let (x, y) = (cursor.0 - win.x, cursor.1 - win.y);
    y >= TOP_STRIP && zones.iter().any(|z| z.contains(x, y))
}

/// The island is open: a card, or the pointer over the ears with agents to list — or, with an
/// agent connected, an empty list («Агенты свободны») so the Drop can still be poked.
pub fn opens(connected: bool, rows: bool, card: bool, hover: bool) -> bool {
    card || (hover && (rows || connected))
}

/// The ears (closed) or the whole open island, in the window's screen frame (top-left points).
pub fn hover_zone(win: Rect, open: bool) -> Zone {
    if open {
        return Zone { x: win.x, y: win.y, w: win.w, h: win.h };
    }
    Zone { x: win.x + (win.w - EARS_W) / 2.0, y: win.y, w: EARS_W, h: EARS_H }
}

// ---------- In the app ----------

pub use glue::*;

mod glue {
    use super::*;
    use crate::agents::{host, summary, wire};
    use crate::app::commands::AppState;
    use crate::app::orchestrator::AppStatus;
    use crate::app::{overlay, overlay_rest, tray};
    use crate::providers::TextModel;
    use tauri::{AppHandle, Emitter, Manager, State};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    /// Managed by the app: the service's state.
    pub struct Agents {
        pub core: Arc<Core>,
        shown: Mutex<Shown>,
        /// What on the island can be clicked (buttons, rows, the Drop), in window points.
        hits: Mutex<Vec<Zone>>,
        /// Woken by pointer moves near the top: the island opens without waiting for a poll.
        wake: Arc<tokio::sync::Notify>,
    }

    /// What the window currently shows for the agents, so only changes go out.
    #[derive(Default)]
    struct Shown {
        view: Option<AgentView>,
        hover: bool,
        clickable: Option<bool>,
        /// The window was brought up for a card while the ears were off (a full-screen app).
        forced: bool,
    }

    fn lang(app: &AppHandle) -> Lang {
        let ui = app.try_state::<AppState>().map(|s| s.shared.settings().ui_language).unwrap_or(crate::settings::UiLanguage::Auto);
        tray::resolve_lang(ui, sys_locale::get_locale().as_deref())
    }

    fn settings(app: &AppHandle) -> Settings {
        app.try_state::<AppState>().map(|s| s.shared.settings()).unwrap_or_default()
    }

    /// The agents part of the settings changed: forget what was switched off, show the rest.
    pub fn settings_changed(app: &AppHandle, s: &AgentSettings) {
        if let Some(agents) = app.try_state::<Agents>() {
            agents.core.forget_disabled(s);
        }
        publish(app);
    }

    /// Sends the view to the island if it changed.
    pub fn publish(app: &AppHandle) {
        let Some(agents) = app.try_state::<Agents>() else { return };
        let view = with_connection(agents.core.view(), &settings(app).agents);
        let mut shown = agents.shown.lock().unwrap_or_else(|e| e.into_inner());
        if shown.view.as_ref() != Some(&view) {
            let _ = app.emit_to(overlay::LABEL, "overlay://agents", &view);
            shown.view = Some(view);
        }
    }

    pub fn spawn(app: AppHandle) {
        let core = Arc::new(Core::default());
        let wake = Arc::new(tokio::sync::Notify::new());
        app.manage(Agents { core: core.clone(), shown: Mutex::new(Shown::default()), hits: Mutex::new(Vec::new()), wake: wake.clone() });
        let _ = app.run_on_main_thread(move || watch_pointer(wake));
        tauri::async_runtime::spawn(listen(app.clone(), core.clone()));
        tauri::async_runtime::spawn(watch(app, core));
    }

    async fn listen(app: AppHandle, core: Arc<Core>) {
        let path = wire::socket_path();
        // A socket file left by a crashed Utter would make bind fail.
        let _ = std::fs::remove_file(&path);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let listener = match tokio::net::UnixListener::bind(&path) {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("agents socket not opened: {e}");
                return;
            }
        };
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        tracing::info!("agents socket open");
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    tauri::async_runtime::spawn(connection(app.clone(), core.clone(), stream));
                }
                // A persistent error (out of file descriptors) must not spin a core (review 2026-11-10).
                Err(e) => {
                    tracing::warn!("agents socket: {e}");
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }

    fn enabled(s: &Settings, source: Source) -> bool {
        match source {
            Source::Claude => s.agents.claude_code,
            Source::Codex => s.agents.codex,
        }
    }

    async fn connection(app: AppHandle, core: Arc<Core>, stream: tokio::net::UnixStream) {
        let (read, mut write) = stream.into_split();
        let mut line = String::new();
        let mut reader = BufReader::new(read.take(LINE_MAX));
        if reader.read_line(&mut line).await.is_err() {
            return;
        }
        let Ok(req) = serde_json::from_str::<Request>(&line) else { return };
        let s = settings(&app);
        let incoming = if enabled(&s, req.source) {
            core.receive(req, lang(&app), Instant::now())
        } else if req.wants_reply {
            Incoming::Reply(Reply::NoDecision)
        } else {
            Incoming::Ignored
        };
        publish(&app);
        let reply = match incoming {
            Incoming::Ignored | Incoming::Updated => None,
            Incoming::Reply(r) => Some(r),
            Incoming::Ask { id, rx } => {
                tracing::info!(id, "agent asks for permission");
                // The helper sends nothing more: anything read here is its end (Esc in Claude
                // Code, its own timeout). Then the card must go (review 2026-11-10).
                let mut rest = [0u8; 1];
                let r = tokio::select! {
                    r = rx => r.unwrap_or(Reply::NoDecision),
                    _ = reader.read(&mut rest) => {
                        core.abandon(id);
                        tracing::info!(id, "the agent stopped waiting");
                        Reply::NoDecision
                    }
                };
                tracing::info!(id, ?r, "permission answered");
                publish(&app);
                Some(r)
            }
            Incoming::Finished { key, last_message } => {
                tauri::async_runtime::spawn(summarize(app.clone(), core.clone(), key, last_message, s));
                None
            }
        };
        if let Some(r) = reply {
            let _ = write.write_all(r.to_line().as_bytes()).await;
        }
    }

    async fn summarize(app: AppHandle, core: Arc<Core>, key: SessionKey, message: String, s: Settings) {
        let model = if s.agents.summary {
            let state = app.try_state::<AppState>();
            let (secrets, providers, budget) = match state {
                Some(st) => (st.secrets.clone(), st.providers.clone(), st.shared.budget.clone()),
                None => return,
            };
            let kind = s.provider;
            // No room left beside the dictation's reserve: the plain summary.
            let room = super::summary_may_use_model(kind, &budget, tokio::time::Instant::now());
            // Reading the key may wait for a Keychain prompt: never on this thread.
            let key_text = tokio::task::spawn_blocking(move || secrets.get(kind)).await.ok().flatten();
            key_text.filter(|_| room).map(|k| providers.make(kind, k))
        } else {
            None
        };
        let started = Instant::now();
        let line = summary::summarize(&message, lang(&app), model.as_deref().map(|p| p as &dyn TextModel), SUMMARY_TIMEOUT).await;
        tracing::info!(ms = started.elapsed().as_millis() as u64, smart = model.is_some(), "agent summary");
        core.set_summary(&key, line);
        publish(&app);
    }

    fn busy(app: &AppHandle) -> bool {
        let pill = app
            .try_state::<overlay_rest::RestPresence>()
            .and_then(|p| p.0.lock().ok().map(|p| p.busy))
            .unwrap_or(false);
        pill || tray::current_status(app) != AppStatus::Idle
    }

    fn ears_shown(app: &AppHandle) -> bool {
        app.try_state::<overlay_rest::RestPresence>().and_then(|p| p.0.lock().ok().map(|p| p.shown)).unwrap_or(false)
    }

    /// The overlay window's frame, read at most once a second: each read waits for the main
    /// thread three times, and the frame changes only when the window is placed.
    fn frame(app: &AppHandle, cache: &Mutex<Option<(Rect, Instant)>>) -> Option<Rect> {
        let cached = *cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((r, at)) = cached {
            if at.elapsed() < Duration::from_secs(1) {
                return Some(r);
            }
        }
        // No lock held while waiting for the main thread.
        let r = window_rect(app)?;
        *cache.lock().unwrap_or_else(|e| e.into_inner()) = Some((r, Instant::now()));
        Some(r)
    }

    /// The overlay window's frame in screen points (top-left).
    fn window_rect(app: &AppHandle) -> Option<Rect> {
        let w = app.get_webview_window(overlay::LABEL)?;
        let (pos, size, scale) = (w.outer_position().ok()?, w.outer_size().ok()?, w.scale_factor().ok()?);
        Some(Rect { x: pos.x as f64 / scale, y: pos.y as f64 / scale, w: size.width as f64 / scale, h: size.height as f64 / scale })
    }

    /// Every 100 ms while there are agents, 500 ms while only the ears are up (pokes at the Drop,
    /// spec §3.6), 1 s otherwise: time for cards and permissions, the pointer over the ears,
    /// whether the window takes clicks.
    /// The watcher never holds `shown` while it calls into Tauri: window getters and setters wait
    /// for the main thread, where the agent commands lock `shown` (review 2026-11-10: deadlock).
    async fn watch(app: AppHandle, core: Arc<Core>) {
        let frame_cache: Mutex<Option<(Rect, Instant)>> = Mutex::new(None);
        loop {
            let Some(agents) = app.try_state::<Agents>() else {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };
            let hover = agents.shown.lock().unwrap_or_else(|e| e.into_inner()).hover;
            let period = watch_period(&core.view(), hover, ears_shown(&app));
            tokio::select! {
                _ = tokio::time::sleep(period) => {}
                _ = agents.wake.notified() => {}
            }
            let idle = !busy(&app);
            core.tick(Instant::now(), idle);
            publish(&app);
            let view = core.view();
            if !idle {
                // The dictation owns the window; take it back when it is over.
                agents.shown.lock().unwrap_or_else(|e| e.into_inner()).clickable = None;
                continue;
            }
            let ears = ears_shown(&app);
            let card = view.card.is_some();
            let connected = with_connection(view.clone(), &settings(&app).agents).connected;
            let open = opens(connected, !view.rows.is_empty(), card, hover);
            let (over, clickable) = if needs_pointer(&view, ears) {
                match (overlay_rest::cursor_point(), frame(&app, &frame_cache)) {
                    (Some(p), Some(win)) => {
                        let over = hover_zone(win, open).contains(p.0, p.1);
                        let hits = agents.hits.lock().unwrap_or_else(|e| e.into_inner()).clone();
                        (over, takes_clicks(win, p, &hits))
                    }
                    _ => (false, false),
                }
            } else {
                (false, false)
            };
            // Decide under the lock, act after it.
            let (hover_changed, clickable_changed, force_show, force_hide) = {
                let mut shown = agents.shown.lock().unwrap_or_else(|e| e.into_inner());
                let hover_changed = over != shown.hover;
                shown.hover = over;
                let clickable_changed = shown.clickable != Some(clickable);
                shown.clickable = Some(clickable);
                // A card must be seen even when the ears are off (a full-screen app).
                let force_show = card && !shown.forced && !ears;
                let force_hide = !card && shown.forced;
                if force_show {
                    shown.forced = true;
                }
                if force_hide {
                    shown.forced = false;
                }
                (hover_changed, clickable_changed, force_show, force_hide)
            };
            if hover_changed {
                tracing::info!(over, "agents hover");
                let _ = app.emit_to(overlay::LABEL, "overlay://agent-hover", serde_json::json!({ "over": over }));
            }
            if clickable_changed {
                if let Some(w) = app.get_webview_window(overlay::LABEL) {
                    let _ = w.set_ignore_cursor_events(!clickable);
                }
            }
            if force_show {
                let a = app.clone();
                let _ = app.run_on_main_thread(move || overlay::show_for_agents(&a));
            } else if force_hide {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(overlay::HIDE_AFTER).await;
                    let b = a.clone();
                    let _ = a.run_on_main_thread(move || {
                        let card_back = b.try_state::<Agents>().is_some_and(|ag| ag.core.view().card.is_some());
                        if !card_back && !ears_shown(&b) && !busy(&b) {
                            overlay::hide_window(&b);
                        }
                    });
                });
            }
        }
    }

    /// Wakes the watcher on every pointer move near the top of a display (and on leaving it), so
    /// hovering the notch opens the island at once (the user 2026-11-10). A global monitor sees
    /// moves over other apps only — exactly where the pointer is when it reaches the notch.
    /// Must run on the main thread.
    #[cfg(target_os = "macos")]
    fn watch_pointer(wake: Arc<tokio::sync::Notify>) {
        use objc2_app_kit::{NSEvent, NSEventMask};
        use std::sync::atomic::{AtomicBool, Ordering};
        let inside = AtomicBool::new(false);
        let block = block2::RcBlock::new(move |_event: std::ptr::NonNull<NSEvent>| {
            let Some((_, y)) = overlay_rest::cursor_point() else { return };
            let top = overlay_rest::cursor_screen().map(|r| r.y).unwrap_or(0.0);
            let (now_inside, look) = pointer_band(y - top, inside.load(Ordering::Relaxed));
            inside.store(now_inside, Ordering::Relaxed);
            if look {
                wake.notify_one();
            }
        });
        let mask = NSEventMask::MouseMoved | NSEventMask::LeftMouseDragged;
        match NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &block) {
            // The monitor lives as long as Utter.
            Some(monitor) => std::mem::forget(monitor),
            None => tracing::warn!("pointer monitor not installed: hover waits for the poll"),
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn watch_pointer(_wake: Arc<tokio::sync::Notify>) {}

    /// «Пауза агентов» from the menu: no cards; open permission requests go back to the agent.
    pub fn set_paused(app: &AppHandle, on: bool) {
        let Some(agents) = app.try_state::<Agents>() else { return };
        agents.core.set_paused(on, Instant::now());
        tracing::info!(on, "agents paused");
        publish(app);
    }

    /// Brings the app the session runs in forward, with its desktop.
    fn open_host(app: &AppHandle, key: &SessionKey) {
        let Some(agents) = app.try_state::<Agents>() else { return };
        let Some(ancestors) = agents.core.ancestors(key) else { return };
        let _ = app.run_on_main_thread(move || {
            let chain = host::parent_chain(&ancestors);
            match host::pick_host(&chain) {
                Some(h) => {
                    tracing::info!(?h, "opening the agent's app");
                    host::activate(&h);
                }
                None => tracing::info!("no app found for the agent session"),
            }
        });
    }

    #[tauri::command]
    pub fn agents_status(state: State<AppState>) -> AgentsStatus {
        status(&state.shared.settings().agents)
    }

    #[tauri::command]
    pub fn agents_view(app: AppHandle, agents: State<Agents>) -> AgentView {
        with_connection(agents.core.view(), &settings(&app).agents)
    }

    #[tauri::command]
    pub fn agent_permission(app: AppHandle, agents: State<Agents>, id: u64, choice: Choice) {
        let key = match agents.core.view().card {
            Some(Card::Permission { id: i, key, .. }) if i == id => Some(key),
            _ => None,
        };
        agents.core.answer(id, choice);
        if choice == Choice::Open {
            if let Some(key) = key {
                open_host(&app, &key);
            }
        }
        publish(&app);
    }

    #[tauri::command]
    pub fn agent_open(app: AppHandle, agents: State<Agents>, key: SessionKey) {
        agents.core.seen(&key);
        open_host(&app, &key);
        publish(&app);
    }

    /// The island page tells where its buttons, rows and the Drop are (window points).
    #[tauri::command]
    pub fn agent_hit_zones(agents: State<Agents>, zones: Vec<Zone>) {
        *agents.hits.lock().unwrap_or_else(|e| e.into_inner()) = zones;
    }

    #[tauri::command]
    pub fn agent_dismiss(app: AppHandle, agents: State<Agents>) {
        agents.core.dismiss(Instant::now());
        publish(&app);
    }

    #[tauri::command]
    pub fn agent_seen(app: AppHandle, agents: State<Agents>, key: SessionKey) {
        agents.core.seen(&key);
        publish(&app);
    }
}

/// An agent summary may use the model only while the dictation's reserve stays free
/// (spec 2026-11-10 §4.5); other providers have no shared budget.
pub fn summary_may_use_model(provider: crate::settings::ProviderKind, budget: &crate::providers::budget::SharedBudget, now: tokio::time::Instant) -> bool {
    use crate::providers::budget::Priority;
    use crate::providers::defaults::GEMINI_TEXT_MODEL;
    provider != crate::settings::ProviderKind::Gemini || budget.lock().is_ok_and(|mut b| b.try_take(GEMINI_TEXT_MODEL, Priority::Agent, now))
}

#[cfg(test)]
mod tests {
    #[tokio::test(start_paused = true)]
    async fn agent_summary_leaves_the_dictation_reserve() {
        use crate::providers::budget::{Priority, RequestBudget};
        use crate::providers::defaults::GEMINI_TEXT_MODEL;
        use crate::settings::ProviderKind;
        let budget = Arc::new(Mutex::new(RequestBudget::new(14, 3)));
        let now = tokio::time::Instant::now();
        assert!(summary_may_use_model(ProviderKind::Gemini, &budget, now));
        for _ in 0..10 {
            assert!(budget.lock().unwrap().try_take(GEMINI_TEXT_MODEL, Priority::Speech, now));
        }
        assert!(!summary_may_use_model(ProviderKind::Gemini, &budget, now), "only the reserve is left");
        assert!(summary_may_use_model(ProviderKind::Groq, &budget, now));
        assert!(budget.lock().unwrap().try_take(GEMINI_TEXT_MODEL, Priority::End, now), "Groq took nothing");
    }

    use super::*;
    use crate::agents::wire::{Request, Source};
    use serde_json::json;

    const HELPER: &str = "/Applications/Utter.app/Contents/MacOS/utter-agent";
    const SKY: &str = "/Applications/OpenAI.app/Contents/Helpers/SkyComputerUseClient";
    const BOTH: Detected = Detected { claude_code: true, codex: true };

    fn home() -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join(".claude")).unwrap();
        std::fs::create_dir(home.path().join(".codex")).unwrap();
        home
    }

    fn on(claude_code: bool, codex: bool) -> AgentSettings {
        AgentSettings { claude_code, codex, summary: true }
    }

    #[test]
    fn apply_settings_on_off_round_trip() {
        let home = home();
        let claude = home.path().join(".claude/settings.json");
        let codex = home.path().join(".codex/config.toml");
        std::fs::write(&claude, "{\n  \"model\": \"opus\"\n}\n").unwrap();
        let codex_before = format!("model = \"gpt-5\"\nnotify = [\"{SKY}\", \"turn-ended\"]\n");
        std::fs::write(&codex, &codex_before).unwrap();

        let (status, prev) = apply_in(home.path(), HELPER, &on(true, true), None, BOTH);
        assert_eq!(status.claude_code, Link::On);
        assert_eq!(status.codex, Link::On);
        assert_eq!(prev, Some(vec![SKY.to_string(), "turn-ended".to_string()]));
        assert!(install::claude_installed(&std::fs::read_to_string(&claude).unwrap(), HELPER));
        assert!(install::codex_installed(&std::fs::read_to_string(&codex).unwrap(), HELPER));
        // The check alone says the same and changes nothing.
        assert_eq!(status_in(home.path(), HELPER, &on(true, true), BOTH), status);

        let (status, prev) = apply_in(home.path(), HELPER, &on(false, false), prev, BOTH);
        assert_eq!((status.claude_code, status.codex), (Link::Off, Link::Off));
        assert_eq!(prev, None);
        let claude_after: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&claude).unwrap()).unwrap();
        assert_eq!(claude_after, json!({"model": "opus"}));
        assert_eq!(std::fs::read_to_string(&codex).unwrap(), codex_before);
    }

    #[test]
    fn creates_missing_agent_files() {
        let home = home();
        let (status, prev) = apply_in(home.path(), HELPER, &on(true, true), None, BOTH);
        assert_eq!((status.claude_code, status.codex), (Link::On, Link::On));
        assert_eq!(prev, None);
        assert!(home.path().join(".claude/settings.json").exists());
        assert!(home.path().join(".codex/config.toml").exists());
    }

    #[test]
    fn broken_file_reports_broken_and_untouched() {
        let home = home();
        let claude = home.path().join(".claude/settings.json");
        std::fs::write(&claude, "{oops").unwrap();
        let (status, _) = apply_in(home.path(), HELPER, &on(true, false), None, BOTH);
        assert!(matches!(status.claude_code, Link::Broken(_)));
        assert_eq!(std::fs::read_to_string(&claude).unwrap(), "{oops");
    }

    #[test]
    fn missing_agent_is_missing_and_untouched() {
        let home = tempfile::tempdir().unwrap();
        let none = Detected { claude_code: false, codex: false };
        let (status, _) = apply_in(home.path(), HELPER, &on(true, true), None, none);
        assert_eq!((status.claude_code, status.codex), (Link::Missing, Link::Missing));
        assert!(!home.path().join(".claude").exists());
        assert!(!home.path().join(".codex").exists());
    }

    #[test]
    fn writes_through_a_symlinked_settings_file() {
        // Dotfile managers keep ~/.claude/settings.json as a link: the link must stay a link.
        let home = home();
        let real = home.path().join("dotfiles-settings.json");
        std::fs::write(&real, "{}").unwrap();
        let link = home.path().join(".claude/settings.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        apply_in(home.path(), HELPER, &on(true, false), None, BOTH);
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(install::claude_installed(&std::fs::read_to_string(&real).unwrap(), HELPER));
    }

    fn request(event: &str, payload: serde_json::Value) -> Request {
        Request { v: 1, source: Source::Claude, event: event.into(), payload, ancestors: vec![10, 1], wants_reply: event == "PermissionRequest" }
    }

    fn permission() -> Request {
        request(
            "PermissionRequest",
            json!({"session_id":"s","cwd":"/p","tool_name":"Bash","tool_input":{"command":"ls"},
                   "permission_suggestions":[{"type":"addRules"}]}),
        )
    }

    #[tokio::test]
    async fn permission_during_dictation_waits_until_idle() {
        let t = Instant::now();
        let core = Core::default();
        let Incoming::Ask { id, rx } = core.receive(permission(), Lang::En, t) else { panic!() };
        // A dictation is on: the card is not on screen, its 30 s do not run.
        core.tick(t + Duration::from_secs(60), false);
        assert!(core.broker.pending().contains(&id));
        // The dictation ended: shown now, 30 s from here.
        core.tick(t + Duration::from_secs(61), true);
        core.tick(t + Duration::from_secs(90), true);
        assert!(core.broker.pending().contains(&id));
        core.tick(t + Duration::from_secs(91), true);
        assert_eq!(rx.await.unwrap(), Reply::NoDecision);
        assert_eq!(core.view().card, None);
    }

    #[tokio::test]
    async fn answer_reaches_helper_and_clears_card() {
        let t = Instant::now();
        let core = Core::default();
        let Incoming::Ask { id, rx } = core.receive(permission(), Lang::En, t) else { panic!() };
        core.answer(id, Choice::Always);
        assert_eq!(rx.await.unwrap(), Reply::Allow { always: true });
        assert_eq!(core.view().card, None);
    }

    #[test]
    fn paused_request_gets_no_decision_at_once() {
        let t = Instant::now();
        let core = Core::default();
        core.set_paused(true, t);
        assert!(matches!(core.receive(permission(), Lang::En, t), Incoming::Reply(Reply::NoDecision)));
    }

    #[tokio::test]
    async fn pausing_answers_open_requests() {
        let t = Instant::now();
        let core = Core::default();
        let Incoming::Ask { rx, .. } = core.receive(permission(), Lang::En, t) else { panic!() };
        core.set_paused(true, t);
        assert_eq!(rx.await.unwrap(), Reply::NoDecision);
    }

    #[test]
    fn pause_item_toggles_registry() {
        let t = Instant::now();
        let core = Core::default();
        core.set_paused(true, t);
        assert!(core.view().paused);
        assert!(core.paused());
        core.set_paused(false, t);
        assert!(!core.view().paused);
        assert!(!core.paused());
    }

    #[tokio::test]
    async fn helper_gone_clears_card() {
        // Review 2026-11-10: Esc in Claude Code (or its own timeout) closes the helper; the card
        // must go and nobody may answer into the void.
        let t = Instant::now();
        let core = Core::default();
        let Incoming::Ask { id, rx } = core.receive(permission(), Lang::En, t) else { panic!() };
        core.abandon(id);
        assert_eq!(rx.await.unwrap(), Reply::NoDecision);
        assert_eq!(core.view().card, None);
        assert!(core.broker.pending().is_empty());
    }

    #[tokio::test]
    async fn ended_session_answers_its_open_requests() {
        let t = Instant::now();
        let core = Core::default();
        let Incoming::Ask { rx, .. } = core.receive(permission(), Lang::En, t) else { panic!() };
        core.receive(request("SessionEnd", json!({"session_id":"s","cwd":"/p"})), Lang::En, t);
        assert_eq!(rx.await.unwrap(), Reply::NoDecision);
        assert!(core.broker.pending().is_empty());
        assert_eq!(core.ancestors(&SessionKey { agent: crate::agents::event::Agent::ClaudeCode, id: "s".into() }), None);
    }

    #[test]
    fn period_is_fast_only_when_needed() {
        use crate::agents::registry::{AgentRow, SessionState};
        let key = SessionKey { agent: crate::agents::event::Agent::Codex, id: "t".into() };
        let row = AgentRow { key: key.clone(), agent: key.agent, project: "p".into(), state: SessionState::Done, steps: None, actions: vec![], summary: None, seen: false };
        let none = AgentView { rows: vec![], card: None, paused: false, connected: false };
        let dots = AgentView { rows: vec![row.clone()], card: None, paused: false, connected: false };
        let card = AgentView { rows: vec![row], card: Some(Card::Done { key }), paused: false, connected: false };
        // An unseen green dot may stay for hours: no 10 Hz polling for it.
        assert_eq!(watch_period(&dots, false, true), Duration::from_millis(300));
        assert_eq!(watch_period(&dots, true, true), Duration::from_millis(100));
        assert_eq!(watch_period(&card, false, true), Duration::from_millis(100));
        assert_eq!(watch_period(&none, false, true), Duration::from_millis(500));
        assert_eq!(watch_period(&none, false, false), Duration::from_millis(1000));
        // Nothing to hover: the pointer is not even read.
        assert!(!needs_pointer(&none, false));
        assert!(needs_pointer(&none, true));
        assert!(needs_pointer(&dots, false));
    }

    #[test]
    fn unstable_helper_paths_are_refused() {
        // Run from the disk image or a translocated copy, the hooks would point at a path that
        // vanishes: every Claude Code event would fail and Codex would lose its own notify.
        assert!(!stable_helper("/Volumes/Utter/Utter.app/Contents/MacOS/utter-agent"));
        assert!(!stable_helper("/private/var/folders/x/T/AppTranslocation/ABC/d/Utter.app/Contents/MacOS/utter-agent"));
        assert!(stable_helper("/Applications/Utter.app/Contents/MacOS/utter-agent"));
        assert!(stable_helper("/Users/me/Desktop/Translate/src-tauri/target/release/bundle/macos/Utter.app/Contents/MacOS/utter-agent"));
    }

    #[test]
    fn write_keeps_file_mode() {
        // ~/.claude/settings.json may hold tokens (env, MCP): a 0600 file stays 0600.
        use std::os::unix::fs::PermissionsExt;
        let home = home();
        let claude = home.path().join(".claude/settings.json");
        std::fs::write(&claude, "{}").unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o600)).unwrap();
        apply_in(home.path(), HELPER, &on(true, false), None, BOTH);
        assert!(install::claude_installed(&std::fs::read_to_string(&claude).unwrap(), HELPER));
        assert_eq!(std::fs::metadata(&claude).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn view_says_whether_an_agent_is_connected() {
        let core = Core::default();
        assert!(!with_connection(core.view(), &on(false, false)).connected);
        assert!(with_connection(core.view(), &on(true, false)).connected);
        assert!(with_connection(core.view(), &on(false, true)).connected);
    }

    #[test]
    fn question_tools_are_answered_at_once() {
        // AskUserQuestion comes as a permission request, but Utter shows it as «ждёт тебя».
        let t = Instant::now();
        let core = Core::default();
        let req = request("PermissionRequest", json!({"session_id":"s","cwd":"/p","tool_name":"AskUserQuestion"}));
        assert!(matches!(core.receive(req, Lang::En, t), Incoming::Reply(Reply::NoDecision)));
    }

    #[test]
    fn finished_asks_for_a_summary_and_keeps_ancestors() {
        let t = Instant::now();
        let core = Core::default();
        let req = request("Stop", json!({"session_id":"s","cwd":"/p","last_assistant_message":"All done."}));
        let Incoming::Finished { key, last_message } = core.receive(req, Lang::En, t) else { panic!() };
        assert_eq!(last_message, "All done.");
        assert_eq!(core.ancestors(&key), Some(vec![10, 1]));
        assert!(matches!(core.receive(request("Nope", json!({"session_id":"s"})), Lang::En, t), Incoming::Ignored));
    }

    #[test]
    fn moves_near_the_top_wake_the_watcher() {
        // The user 2026-11-10: the island must open the moment the pointer reaches the notch,
        // not on the next poll. Moves in the band wake the watcher, and so does leaving it.
        assert_eq!(pointer_band(10.0, false), (true, true));
        assert_eq!(pointer_band(BAND - 1.0, true), (true, true));
        assert_eq!(pointer_band(BAND + 50.0, true), (false, true));
        assert_eq!(pointer_band(BAND + 50.0, false), (false, false));
    }

    #[test]
    fn only_the_buttons_take_clicks_never_the_top_strip() {
        // The user 2026-11-10: a window taking clicks where a full-screen app shows its menu and
        // title bar kept them from hiding — first at the very top, then over the open island.
        // Clicks only right over a button, a list row or the Drop, and never in the top strip.
        let win = Rect { x: 100.0, y: 0.0, w: 420.0, h: 206.0 };
        let button = Zone { x: 84.0, y: 120.0, w: 80.0, h: 28.0 };
        let high = Zone { x: 84.0, y: 10.0, w: 80.0, h: 40.0 };
        assert!(takes_clicks(win, (200.0, 130.0), &[button]));
        assert!(!takes_clicks(win, (200.0, 100.0), &[button]));
        assert!(!takes_clicks(win, (300.0, 130.0), &[button]));
        assert!(!takes_clicks(win, (200.0, TOP_STRIP - 1.0), &[high]));
        assert!(takes_clicks(win, (200.0, TOP_STRIP), &[high]));
        assert!(!takes_clicks(win, (200.0, 130.0), &[]));
    }

    #[test]
    fn hovering_bare_ears_opens_the_island_when_an_agent_is_connected() {
        assert!(opens(true, false, false, true));
        assert!(!opens(false, false, false, true));
        assert!(opens(false, true, false, true));
        assert!(opens(false, false, true, false));
        assert!(!opens(true, true, false, false));
    }

    #[test]
    fn hover_zone_is_the_ears_or_the_open_island() {
        let win = Rect { x: 100.0, y: 0.0, w: 420.0, h: 206.0 };
        let ears = hover_zone(win, false);
        assert!(ears.contains(310.0, 10.0));
        assert!(!ears.contains(310.0, 60.0));
        assert!(!ears.contains(110.0, 10.0));
        let open = hover_zone(win, true);
        assert!(open.contains(110.0, 150.0));
        assert!(!open.contains(90.0, 10.0));
    }
}
