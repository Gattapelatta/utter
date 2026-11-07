//! Wires hotkeys → recorder → pipeline → output. One task owns all mutable state;
//! everything that talks to the OS sits behind a trait so the flow is testable.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use tokio::sync::mpsc::UnboundedSender;

use crate::audio::{self, dsp, segment, Captured};
use crate::controller::{Command, Controller, HotkeyEvent, State};
use crate::history::{EntryStatus, History, HistoryEntry};
use crate::jobs::{JobIds, OrderedDelivery};
use crate::live::run::{self as live_run, Finisher, LiveHandle, LiveOutcome, LiveRequest};
use crate::output::{CopyReason, DeliveryOutcome};
use crate::permissions::PermStatus;
use crate::pipeline::routes::{self, AttemptOutcome, Route, RouteHealth};
use crate::pipeline::{self, JobError, JobOutput, PipelineOptions};
use crate::providers::gemini_live::{LiveConfig, LiveConnector, TranscribeMode};
use crate::providers::{defaults, AudioLlm, ProviderError, SpeechToText, TextModel};
use crate::recorder::AudioSource;
use crate::secrets::SecretStore;
use crate::settings::{Hotkeys, InsertMethod, ProviderKind, Settings, TranslateStyle};
use crate::types::Mode;
use crate::usage::Usage;

/// Long enough for the Drop's success animation (§4.2).
const DONE_VISIBLE: Duration = Duration::from_millis(2200);
/// Long enough for the clouds to clear (§4.2).
const ERROR_VISIBLE: Duration = Duration::from_millis(3000);
/// A wall-clock gap this long between 100 ms ticks means the machine was asleep.
const WAKE_GAP: Duration = Duration::from_secs(10);
const HOTKEY_CHECK: Duration = Duration::from_secs(2);
/// How often a running recording is checked for a pause to cut a piece at.
const PIECE_CHECK: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OverlayState {
    Hidden,
    Recording { mode: Mode, style: TranslateStyle, started_at_ms: i64, warn: bool, pending: u32 },
    Processing { pending: u32, since_ms: i64 },
    Done { outcome: String, degraded: bool },
    Error { message_key: String, action: Option<String>, minutes: Option<u32>, provider: Option<String> },
    /// Recording cancelled: the pill fades out quickly.
    Cancelled,
}

impl OverlayState {
    pub fn kind(&self) -> &'static str {
        match self {
            OverlayState::Hidden => "hidden",
            OverlayState::Recording { .. } => "recording",
            OverlayState::Processing { .. } => "processing",
            OverlayState::Done { .. } => "done",
            OverlayState::Error { .. } => "error",
            OverlayState::Cancelled => "cancelled",
        }
    }

    fn error(key: &str, action: Option<&str>) -> Self {
        OverlayState::Error { message_key: key.into(), action: action.map(Into::into), minutes: None, provider: None }
    }
}

/// What the app is doing, for the tray icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppStatus {
    Idle,
    Recording,
    Processing,
}

/// The onboarding trial recording's result, shown in the settings window.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrialOutcome {
    Text { mode: Mode, text: String },
    Error { message_key: String },
}

/// Events for the settings window and the tray.
#[derive(Clone, Debug, PartialEq)]
pub enum UiEvent {
    Status(AppStatus),
    Hotkey { mode: Mode, down: bool },
    Trial(TrialOutcome),
    HistoryChanged,
}

pub trait UiSink: Send + Sync {
    fn emit(&self, e: UiEvent);
}


pub trait OverlaySink: Send + Sync {
    fn show(&self, s: &OverlayState);
    /// The live draft for the island's window; `None` removes it.
    fn draft(&self, _parts: Option<&crate::live::draft::DraftParts>) {}
}

pub trait Deliverer: Send + Sync {
    fn deliver(&self, text: &str, method: InsertMethod) -> DeliveryOutcome;
}

pub trait PermissionCheck: Send + Sync {
    fn accessibility(&self) -> bool;
    fn microphone(&self) -> PermStatus;
}

pub trait HotkeyControl: Send + Sync {
    fn set_bindings(&self, b: Hotkeys);
    /// While on, Escape and chord keys are consumed instead of reaching the focused app.
    fn set_recording(&self, on: bool);
    /// Starts the listener if it is not running (e.g. permission was just granted).
    fn ensure_running(&self) -> bool;
    fn restart(&self);
}

#[async_trait::async_trait]
pub trait Provider: SpeechToText + TextModel {
    async fn prewarm(&self);
    async fn verify_key(&self) -> Result<(), ProviderError>;
    /// A one-request audio→text model, when the provider has one.
    fn direct(&self) -> Option<&dyn AudioLlm> {
        None
    }
}

pub trait ProviderFactory: Send + Sync {
    fn make(&self, kind: ProviderKind, key: String) -> Arc<dyn Provider>;
    /// A client for one backup route (spec 2026-11-04 §4.5).
    fn make_route(&self, kind: ProviderKind, key: String, _route: Route) -> Arc<dyn Provider> {
        self.make(kind, key)
    }
    /// Fresh connections for the clients made from now on (after sleep an old one can be dead).
    fn reset(&self) {}
}

/// The provider for jobs without an LLM step and its backup routes, in priority order
/// (none for providers with a single route).
#[derive(Clone)]
struct Backend {
    single: Arc<dyn Provider>,
    routes: Vec<(Route, Arc<dyn Provider>)>,
    health: Arc<Mutex<RouteHealth>>,
}

pub struct Deps {
    pub secrets: Arc<dyn SecretStore>,
    pub audio: Arc<dyn AudioSource>,
    pub providers: Arc<dyn ProviderFactory>,
    pub output: Arc<dyn Deliverer>,
    pub overlay: Arc<dyn OverlaySink>,
    pub permissions: Arc<dyn PermissionCheck>,
    pub hotkeys: Arc<dyn HotkeyControl>,
    pub ui: Arc<dyn UiSink>,
    /// The Live API for the draft while speaking (spec 2026-11-06); `None` keeps every
    /// recording on the batch path.
    pub live: Option<Arc<dyn LiveConnector>>,
}

/// State the settings UI reads and writes directly.
pub struct Shared {
    pub settings: Mutex<Settings>,
    pub history: Mutex<History>,
    pub usage: Mutex<Usage>,
    pub data_dir: PathBuf,
    /// Onboarding trial: results go to the settings window instead of being pasted.
    pub trial: AtomicBool,
}

impl Shared {
    pub fn new(settings: Settings, data_dir: PathBuf) -> Self {
        Self {
            history: Mutex::new(History::load(&data_dir.join("history.json"))),
            usage: Mutex::new(Usage::load(&data_dir.join("usage.json"))),
            settings: Mutex::new(settings),
            data_dir,
            trial: AtomicBool::new(false),
        }
    }

    pub fn audio_dir(&self) -> PathBuf {
        self.data_dir.join("pending-audio")
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Deletes pending audio that no history entry references (left by a crash).
    pub fn remove_orphan_audio(&self) {
        let referenced: Vec<PathBuf> = self
            .history
            .lock()
            .map(|h| h.entries().iter().filter_map(|e| e.audio_file.clone()).collect())
            .unwrap_or_default();
        if let Ok(dir) = std::fs::read_dir(self.audio_dir()) {
            for f in dir.flatten() {
                if !referenced.contains(&f.path()) {
                    let _ = std::fs::remove_file(f.path());
                }
            }
        }
    }
}

pub struct JobResult {
    mode: Mode,
    submitted: Instant,
    /// Set when this job retries an existing history entry.
    history_id: Option<u64>,
    audio_path: Option<PathBuf>,
    audio_secs: f32,
    created_at_ms: i64,
    /// Speech requests made (more than one when the recording was sent in pieces).
    requests: u32,
    outcome: Result<JobOutput, JobError>,
}

/// A piece's result and the requests it made (backup routes add attempts).
type Piece = tokio::task::JoinHandle<(Result<JobOutput, JobError>, u32)>;

/// A recording whose first pieces were already sent while the user was speaking.
struct Split {
    pieces: Vec<Piece>,
    rest: Captured,
}

pub enum Input {
    Hotkey(HotkeyEvent),
    Tick,
    JobDone(u64, Box<JobResult>),
    HideOverlay(u64),
    SettingsChanged,
    Retry(u64),
    /// A live dictation (by its sequence number) has its outcome.
    LiveDone(u64, Box<LiveOutcome>),
}

/// The live dictation of the current recording.
struct LiveRec {
    seq: u64,
    handle: LiveHandle,
    /// Interleaved samples already streamed.
    offset: usize,
    layout: (u32, u16),
    /// The batch provider was warmed up after the live connection failed.
    prewarmed: bool,
    /// The stream missed audio (the microphone changed mid-dictation): the live text would be
    /// cut off, so the whole recording takes the batch path at release (review 2026-11-07).
    lost: bool,
}

/// A released live dictation waiting for its outcome.
struct LiveJob {
    id: u64,
    mode: Mode,
    captured: Arc<Captured>,
    submitted: Instant,
    created_at_ms: i64,
    /// Kept alive until the outcome: dropping it cancels the dictation.
    _handle: LiveHandle,
}

impl LiveJob {
    fn secs(&self) -> f32 {
        let per_sec = self.captured.rate as f32 * self.captured.channels.max(1) as f32;
        if per_sec > 0.0 { self.captured.samples.len() as f32 / per_sec } else { 0.0 }
    }

    fn result(&self, outcome: Result<JobOutput, JobError>, requests: u32) -> JobResult {
        JobResult {
            mode: self.mode,
            submitted: self.submitted,
            history_id: None,
            audio_path: None,
            audio_secs: self.secs(),
            created_at_ms: self.created_at_ms,
            requests,
            outcome,
        }
    }
}

enum AudioInput {
    Captured(Captured),
    Wav { bytes: Vec<u8>, secs: f32 },
}

pub struct Orchestrator {
    deps: Deps,
    shared: Arc<Shared>,
    tx: UnboundedSender<Input>,
    controller: Controller,
    ids: JobIds,
    queue: OrderedDelivery<JobResult>,
    in_flight: u32,
    overlay_seq: u64,
    recording: Option<(Mode, i64)>,
    /// A result that finished while a recording was on screen; shown when it ends.
    deferred: Option<(OverlayState, Duration)>,
    /// When the oldest unfinished job started processing (for the "slow" hint).
    processing_since: i64,
    provider: Option<(ProviderKind, String, Backend)>,
    /// Routes that were slow or failing lately go last; outlives provider changes.
    route_health: Arc<Mutex<RouteHealth>>,
    /// `Instant` stops during sleep on macOS, so the tick gap is measured in wall time.
    wall_clock: Box<dyn Fn() -> SystemTime + Send>,
    last_tick_wall: Option<SystemTime>,
    last_hotkey_check: Option<Instant>,
    status: AppStatus,
    /// Pieces of the current recording already sent while the user speaks.
    pieces: Vec<Piece>,
    /// Interleaved samples of the current recording already sent as pieces.
    piece_offset: usize,
    piece_layout: Option<(u32, u16)>,
    last_piece_check: Option<Instant>,
    live: Option<LiveRec>,
    live_seq: u64,
    /// The live dictation whose drafts may reach the island; a released one may still be
    /// sending its last pieces when the next recording starts.
    draft_owner: Arc<std::sync::atomic::AtomicU64>,
    /// Job id → live dictation, for jobs that showed a draft window.
    live_ids: std::collections::HashMap<u64, u64>,
    live_jobs: std::collections::HashMap<u64, LiveJob>,
}

fn now_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

pub fn provider_name(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Gemini => "Gemini",
        ProviderKind::Groq => "Groq",
        ProviderKind::OpenAi => "OpenAI",
    }
}

fn error_state(e: &ProviderError, kind: ProviderKind) -> OverlayState {
    let mut state = match e {
        ProviderError::Unauthorized => OverlayState::error("bad_key", Some("open_provider")),
        ProviderError::RateLimited { retry_after } => OverlayState::Error {
            message_key: "rate_limited".into(),
            action: None,
            minutes: retry_after.map(|d| d.as_secs().div_ceil(60).max(1) as u32),
            provider: None,
        },
        ProviderError::Network => OverlayState::error("network", None),
        // The connection is fine but no answer came in time (spec 2026-11-04 §4.3).
        ProviderError::Timeout => OverlayState::error("slow_provider", None),
        ProviderError::BadResponse(m) if m.to_lowercase().contains("location is not supported") => {
            OverlayState::error("region_blocked", None)
        }
        ProviderError::Server(_) | ProviderError::BadResponse(_) => OverlayState::error("service_down", None),
    };
    if let OverlayState::Error { provider, .. } = &mut state {
        *provider = Some(provider_name(kind).to_string());
    }
    state
}

fn outcome_name(o: DeliveryOutcome) -> &'static str {
    match o {
        DeliveryOutcome::Pasted => "pasted",
        DeliveryOutcome::Typed => "typed",
        DeliveryOutcome::CopiedOnly(CopyReason::InputFailed | CopyReason::OwnWindow | CopyReason::Elevated) => "copied",
        DeliveryOutcome::CopiedOnly(CopyReason::ClipboardFailed) => "failed",
    }
}

fn remove_files(paths: Vec<PathBuf>) {
    for p in paths {
        let _ = std::fs::remove_file(p);
    }
}

impl Orchestrator {
    pub fn new(deps: Deps, shared: Arc<Shared>, tx: UnboundedSender<Input>) -> Self {
        let mode = shared.settings().record_mode;
        Self {
            deps,
            shared,
            tx,
            controller: Controller::new(mode),
            ids: JobIds::default(),
            queue: OrderedDelivery::new(1),
            in_flight: 0,
            overlay_seq: 0,
            recording: None,
            deferred: None,
            processing_since: 0,
            provider: None,
            route_health: Arc::new(Mutex::new(RouteHealth::new(defaults::GEMINI_ROUTES.len()))),
            wall_clock: Box::new(SystemTime::now),
            last_tick_wall: None,
            last_hotkey_check: None,
            status: AppStatus::Idle,
            pieces: Vec::new(),
            piece_offset: 0,
            piece_layout: None,
            last_piece_check: None,
            live: None,
            live_seq: 0,
            draft_owner: Default::default(),
            live_ids: Default::default(),
            live_jobs: Default::default(),
        }
    }

    pub fn controller_state(&self) -> &State {
        self.controller.state()
    }

    /// Wake detection needs a clock that keeps running during sleep (tests inject one).
    pub fn set_wall_clock(&mut self, clock: Box<dyn Fn() -> SystemTime + Send>) {
        self.wall_clock = clock;
    }

    pub fn set_provider_factory(&mut self, f: Arc<dyn ProviderFactory>) {
        self.deps.providers = f;
        self.provider = None;
    }

    /// Main loop: ticks every 100 ms and handles inputs until the channel closes.
    pub async fn run(mut self, mut rx: tokio::sync::mpsc::UnboundedReceiver<Input>) {
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        loop {
            tokio::select! {
                input = rx.recv() => match input {
                    Some(i) => self.handle(i, Instant::now()).await,
                    None => break,
                },
                _ = tick.tick() => self.handle(Input::Tick, Instant::now()).await,
            }
        }
    }

    pub async fn handle(&mut self, input: Input, now: Instant) {
        self.handle_input(input, now).await;
        let status = if self.recording.is_some() {
            AppStatus::Recording
        } else if self.in_flight > 0 {
            AppStatus::Processing
        } else {
            AppStatus::Idle
        };
        if status != self.status {
            self.status = status;
            self.deps.ui.emit(UiEvent::Status(status));
        }
    }

    async fn handle_input(&mut self, input: Input, now: Instant) {
        match input {
            Input::Hotkey(ev) => {
                match ev {
                    HotkeyEvent::Down(mode) => self.deps.ui.emit(UiEvent::Hotkey { mode, down: true }),
                    HotkeyEvent::Up(mode) => self.deps.ui.emit(UiEvent::Hotkey { mode, down: false }),
                    HotkeyEvent::Tap(mode) => {
                        self.deps.ui.emit(UiEvent::Hotkey { mode, down: true });
                        self.deps.ui.emit(UiEvent::Hotkey { mode, down: false });
                    }
                    HotkeyEvent::Escape | HotkeyEvent::OtherKey => {}
                }
                let cmds = self.controller.handle(ev, now);
                self.exec(cmds).await;
            }
            Input::Tick => self.tick(now).await,
            Input::JobDone(id, result) => {
                self.in_flight = self.in_flight.saturating_sub(1);
                if self.in_flight > 0 {
                    // Approximation: the next job's wait is measured from now.
                    self.processing_since = now_ms();
                }
                for (id, r) in self.queue.push(id, *result) {
                    self.finish(r, id).await;
                }
            }
            Input::LiveDone(seq, outcome) => self.live_done(seq, *outcome),
            Input::HideOverlay(seq) => {
                if seq == self.overlay_seq && self.recording.is_none() {
                    if self.in_flight > 0 {
                        // Other jobs are still running: go back to the processing pill.
                        self.show(self.processing_state());
                    } else {
                        self.deps.overlay.show(&OverlayState::Hidden);
                    }
                }
            }
            Input::SettingsChanged => {
                let s = self.shared.settings();
                self.controller.set_record_mode(s.record_mode);
                self.deps.hotkeys.set_bindings(s.hotkeys.clone());
                self.provider = None;
            }
            Input::Retry(id) => self.retry(id),
        }
    }

    async fn tick(&mut self, now: Instant) {
        let wall = (self.wall_clock)();
        if let Some(last) = self.last_tick_wall {
            if wall.duration_since(last).is_ok_and(|gap| gap > WAKE_GAP) {
                tracing::info!("woke from sleep; restarting hotkeys and HTTP client");
                self.deps.hotkeys.restart();
                self.deps.providers.reset();
                self.provider = None;
            }
        }
        self.last_tick_wall = Some(wall);
        let due = self.last_hotkey_check.is_none_or(|t| now.saturating_duration_since(t) >= HOTKEY_CHECK);
        if due {
            self.last_hotkey_check = Some(now);
            self.deps.hotkeys.ensure_running();
        }
        let cmds = self.controller.tick(now);
        self.exec(cmds).await;
        self.stream_live();
        self.check_pieces(now);
    }

    /// Sends what was recorded since the last tick to the live dictation.
    fn stream_live(&mut self) {
        let failed = self.live.as_ref().is_some_and(|l| l.handle.failed() && !l.prewarmed);
        if failed {
            // The pieces path takes over (`check_pieces`); its first request finds a warm connection.
            let s = self.shared.settings();
            if let Some(b) = self.provider_for(&s) {
                tokio::spawn(async move { b.single.prewarm().await });
            }
            if let Some(l) = self.live.as_mut() {
                l.prewarmed = true;
            }
        }
        let Some(l) = self.live.as_mut() else { return };
        if l.lost {
            return;
        }
        // No snapshot while recording: the device changed and the recorder started a new segment.
        let Some(c) = self.deps.audio.peek(l.offset) else {
            tracing::info!("live: the microphone changed mid-dictation; the whole recording takes the batch path");
            l.lost = true;
            return;
        };
        if (c.rate, c.channels) != l.layout {
            l.lost = true;
        } else if !c.samples.is_empty() {
            l.handle.audio(&c.samples);
            l.offset += c.samples.len();
        }
    }

    /// Sends the finished part of a long recording at a pause, while the user keeps speaking.
    fn check_pieces(&mut self, now: Instant) {
        let Some((mode, _)) = self.recording else { return };
        // A live dictation already streams everything (unless its connection failed).
        if self.live.as_ref().is_some_and(|l| !l.handle.failed()) {
            return;
        }
        if self.last_piece_check.is_some_and(|t| now.saturating_duration_since(t) < PIECE_CHECK) {
            return;
        }
        self.last_piece_check = Some(now);
        let s = self.shared.settings();
        // Paragraphs are split by meaning over the whole text, so keep one request.
        if s.paragraphs {
            return;
        }
        let Some(c) = self.deps.audio.peek(self.piece_offset) else { return };
        if c.channels == 0 || self.piece_layout.is_some_and(|l| l != (c.rate, c.channels)) {
            return;
        }
        let mono = dsp::downmix(&c.samples, c.channels);
        let Some(cut) = segment::find_cut(&mono, c.rate) else { return };
        let n = (cut * c.channels as usize).min(c.samples.len());
        let piece = Captured { samples: c.samples[..n].to_vec(), rate: c.rate, channels: c.channels };
        self.piece_offset += n;
        self.piece_layout = Some((c.rate, c.channels));
        tracing::info!(secs = cut as f32 / c.rate as f32, "piece sent while speaking");
        let provider = self.provider_for(&s);
        let opts = pipeline_options(&s);
        self.pieces.push(tokio::spawn(async move { process_capture(mode, piece, opts, provider).await }));
    }

    fn drop_pieces(&mut self) {
        for p in self.pieces.drain(..) {
            p.abort();
        }
        self.piece_offset = 0;
        self.piece_layout = None;
        self.last_piece_check = None;
    }

    fn processing_state(&self) -> OverlayState {
        OverlayState::Processing { pending: self.in_flight, since_ms: self.processing_since }
    }

    fn show(&mut self, s: OverlayState) {
        self.overlay_seq += 1;
        self.deps.overlay.show(&s);
    }

    /// Shows a transient result unless a recording is on screen, then hides it later.
    fn show_result(&mut self, s: OverlayState, visible: Duration) {
        if self.recording.is_some() {
            if matches!(s, OverlayState::Error { .. }) {
                self.deferred = Some((s, visible));
            }
            return;
        }
        if self.in_flight > 0 && matches!(s, OverlayState::Done { .. }) {
            self.show(self.processing_state());
            return;
        }
        self.show(s);
        let seq = self.overlay_seq;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(visible).await;
            let _ = tx.send(Input::HideOverlay(seq));
        });
    }

    fn provider_for(&mut self, s: &Settings) -> Option<Backend> {
        let key = self.deps.secrets.get(s.provider)?;
        if let Some((kind, k, b)) = &self.provider {
            if *kind == s.provider && *k == key {
                return Some(b.clone());
            }
        }
        let f = &self.deps.providers;
        let b = Backend {
            single: f.make(s.provider, key.clone()),
            routes: defaults::routes_for(s.provider)
                .iter()
                .map(|&r| (r, f.make_route(s.provider, key.clone(), r)))
                .collect(),
            health: self.route_health.clone(),
        };
        self.provider = Some((s.provider, key, b.clone()));
        Some(b)
    }

    async fn exec(&mut self, cmds: Vec<Command>) {
        for cmd in cmds {
            match cmd {
                Command::Start(mode) => self.start(mode),
                Command::Submit { mode, .. } => self.submit(mode),
                Command::Cancel(reason) => {
                    tracing::info!("recording cancelled: {reason:?}");
                    self.deps.audio.cancel();
                    self.drop_pieces();
                    if let Some(l) = self.live.take() {
                        l.handle.cancel();
                    }
                    self.deps.overlay.draft(None);
                    if let Some((state, visible)) = self.end_recording() {
                        self.show_result(state, visible);
                    } else if self.in_flight > 0 {
                        self.show(self.processing_state());
                    } else {
                        self.show(OverlayState::Cancelled);
                    }
                }
                Command::LimitWarning => {
                    if let Some((mode, started)) = self.recording {
                        let style = self.shared.settings().translate_style;
                        let pending = self.in_flight;
                        self.show(OverlayState::Recording { mode, style, started_at_ms: started, warn: true, pending });
                    }
                }
            }
        }
    }

    /// Clears recording state; returns a deferred error to show, if any.
    fn end_recording(&mut self) -> Option<(OverlayState, Duration)> {
        self.recording = None;
        self.deps.hotkeys.set_recording(false);
        self.deferred.take()
    }

    fn fail_start(&mut self, state: OverlayState) {
        self.drop_pieces();
        self.controller.abort();
        self.end_recording();
        self.show_result(state, ERROR_VISIBLE);
    }

    fn start(&mut self, mode: Mode) {
        let s = self.shared.settings();
        let Some(provider) = self.provider_for(&s) else {
            return self.fail_start(OverlayState::error("no_key", Some("open_provider")));
        };
        if !self.deps.permissions.accessibility() {
            return self.fail_start(OverlayState::error("no_accessibility", Some("open_permissions")));
        }
        // Denied access yields silent audio from the OS; say so instead of "nothing heard".
        if self.deps.permissions.microphone() == PermStatus::Denied {
            return self.fail_start(OverlayState::error("no_microphone_access", Some("open_permissions")));
        }
        if let Err(e) = self.deps.audio.start() {
            tracing::warn!("recorder failed to start: {e}");
            return self.fail_start(OverlayState::error("no_microphone", None));
        }
        self.drop_pieces();
        if let Some(l) = self.live.take() {
            l.handle.cancel();
        }
        self.draft_owner.store(0, Ordering::SeqCst);
        let started = now_ms();
        self.recording = Some((mode, started));
        self.deps.hotkeys.set_recording(true);
        let pending = self.in_flight;
        self.show(OverlayState::Recording {
            mode,
            style: s.translate_style,
            started_at_ms: started,
            warn: false,
            pending,
        });
        self.live = self.start_live(mode, &s);
        if self.live.is_none() {
            tokio::spawn(async move { provider.single.prewarm().await });
        }
    }

    /// The live path: Gemini, no paragraphs (they split the whole text by meaning), and a recorder
    /// that reports its format. A busy queue is fine: the job id is taken at release, so the
    /// ordered queue keeps the insertion order (user's decision 2026-11-07).
    fn start_live(&mut self, mode: Mode, s: &Settings) -> Option<LiveRec> {
        let connector = self.deps.live.clone()?;
        if s.provider != ProviderKind::Gemini || s.paragraphs {
            return None;
        }
        let c = self.deps.audio.peek(0)?;
        if c.channels == 0 || c.rate == 0 {
            return None;
        }
        let key = self.deps.secrets.get(s.provider)?;
        let finisher = match live_system(mode, &pipeline_options(s)) {
            None => Finisher::Plain,
            Some(system) => {
                let route = Route::Direct { model: defaults::GEMINI_TRANSLATE_MODEL };
                let model: Arc<dyn TextModel> = self.deps.providers.make_route(s.provider, key.clone(), route);
                // The other Lite model (own quota) races a slow answer (live check 2026-11-07).
                let backup = Route::Direct { model: defaults::GEMINI_BACKUP_LITE_MODEL };
                let backup: Arc<dyn TextModel> = self.deps.providers.make_route(s.provider, key.clone(), backup);
                Finisher::Text { model, system, backup: Some(backup) }
            }
        };
        let cfg = LiveConfig {
            url: defaults::GEMINI_LIVE_URL.into(),
            key,
            model: defaults::GEMINI_LIVE_MODEL.into(),
            mode: TranscribeMode::Verbatim,
            language: s.speech_language.clone(),
        };
        self.live_seq += 1;
        let seq = self.live_seq;
        self.draft_owner.store(seq, Ordering::SeqCst);
        let owner = self.draft_owner.clone();
        let overlay = self.deps.overlay.clone();
        let tx = self.tx.clone();
        let handle = live_run::spawn(
            LiveRequest { cfg, finisher, rate: c.rate, channels: c.channels },
            connector,
            Arc::new(move |p| {
                if owner.load(Ordering::SeqCst) == seq {
                    overlay.draft(p.as_ref());
                }
            }),
            move |o| {
                let _ = tx.send(Input::LiveDone(seq, Box::new(o)));
            },
        );
        handle.audio(&c.samples);
        Some(LiveRec { seq, handle, offset: c.samples.len(), layout: (c.rate, c.channels), prewarmed: false, lost: false })
    }

    fn submit_live(&mut self, l: LiveRec, mode: Mode) {
        let deferred = self.end_recording();
        let captured = self.deps.audio.stop().unwrap_or_default();
        if (captured.rate, captured.channels) == l.layout && captured.samples.len() > l.offset {
            l.handle.audio(&captured.samples[l.offset..]);
        }
        l.handle.release();
        self.drop_pieces();
        let id = self.ids.next();
        if self.in_flight == 0 {
            self.processing_since = now_ms();
        }
        self.in_flight += 1;
        self.show(self.processing_state());
        self.live_ids.insert(id, l.seq);
        let job = LiveJob { id, mode, captured: Arc::new(captured), submitted: Instant::now(), created_at_ms: now_ms(), _handle: l.handle };
        self.live_jobs.insert(l.seq, job);
        if let Some((state, visible)) = deferred {
            self.show_result(state, visible);
        }
    }

    fn live_done(&mut self, seq: u64, outcome: LiveOutcome) {
        let Some(job) = self.live_jobs.remove(&seq) else { return };
        match outcome {
            LiveOutcome::Final { text, raw, requests } => {
                let out = JobOutput { text, raw, degraded: false, tokens: 0, llm_used: requests > 0, direct: false };
                let _ = self.tx.send(Input::JobDone(job.id, Box::new(job.result(Ok(out), 1 + requests))));
            }
            LiveOutcome::Unfinished { raw, requests } => self.finish_text(job, raw, requests),
            // The batch path with the recorded audio; a cancel after release cannot happen, but
            // never lose a recording if it does.
            LiveOutcome::Fallback { .. } | LiveOutcome::Cancelled => {
                let captured = Arc::try_unwrap(job.captured).unwrap_or_else(|c| (*c).clone());
                self.run_job(job.id, job.mode, AudioInput::Captured(captured), None, None);
            }
        }
    }

    /// Cleanup or translation of the verbatim text did not come while speaking: one request over
    /// the backup routes; failing that, Dictation pastes the verbatim text, Translate takes the
    /// batch path.
    fn finish_text(&mut self, job: LiveJob, raw: String, requests: u32) {
        let s = self.shared.settings();
        let opts = pipeline_options(&s);
        let backend = self.provider_for(&s);
        let system = live_system(job.mode, &opts);
        let audio_dir = self.shared.audio_dir();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let (mode, captured) = (job.mode, job.captured.clone());
            // A bug here must fail this job only, never stall the ordered queue.
            let work = tokio::spawn(finish_text_work(mode, captured, raw, requests, system, opts, backend));
            let (outcome, n) = work.await.unwrap_or_else(|e| {
                tracing::error!("live job {} crashed: {e}", job.id);
                (Err(JobError::Provider(ProviderError::BadResponse("internal error".into()))), requests)
            });
            let mut result = job.result(outcome, 1 + n);
            if matches!(result.outcome, Err(JobError::Provider(_))) {
                // Kept for Retry in History, as on the batch path.
                result.audio_path = save_audio(&audio_dir, job.created_at_ms, job.id, &job.captured).await;
            }
            let _ = tx.send(Input::JobDone(job.id, Box::new(result)));
        });
    }

    fn submit(&mut self, mode: Mode) {
        let mut window = None;
        if let Some(l) = self.live.take() {
            if !l.handle.failed() && !l.lost {
                return self.submit_live(l, mode);
            }
            // The live connection failed (or missed audio) while speaking: the batch path with its
            // pieces; the window keeps what came until that text is pasted (`finish` clears it).
            l.handle.cancel();
            window = Some(l.seq);
        }
        let deferred = self.end_recording();
        let captured = self.deps.audio.stop().unwrap_or_default();
        let pieces = std::mem::take(&mut self.pieces);
        let paragraphs = self.shared.settings().paragraphs;
        let fits = self.piece_layout == Some((captured.rate, captured.channels)) && captured.samples.len() >= self.piece_offset;
        let split = if paragraphs || (!pieces.is_empty() && !fits) {
            for p in pieces {
                p.abort();
            }
            None
        } else {
            let rest = Captured { samples: captured.samples[self.piece_offset.min(captured.samples.len())..].to_vec(), ..captured.clone() };
            Some(Split { pieces, rest })
        };
        self.drop_pieces();
        let id = self.spawn_job(mode, AudioInput::Captured(captured), None, split);
        if let Some(seq) = window {
            self.live_ids.insert(id, seq);
        }
        if let Some((state, visible)) = deferred {
            self.show_result(state, visible);
        }
    }

    fn retry(&mut self, history_id: u64) {
        let entry = self.shared.history.lock().ok().and_then(|h| h.get(history_id).cloned());
        let Some(entry) = entry else { return };
        let Some(path) = entry.audio_file.clone() else { return };
        let Ok(bytes) = std::fs::read(&path) else { return };
        // 16 kHz mono 16-bit: 32 000 bytes per second after the 44-byte header.
        let secs = bytes.len().saturating_sub(44) as f32 / 32_000.0;
        self.spawn_job(entry.mode, AudioInput::Wav { bytes, secs }, Some(history_id), None);
    }

    fn spawn_job(&mut self, mode: Mode, audio_in: AudioInput, history_id: Option<u64>, split: Option<Split>) -> u64 {
        let id = self.ids.next();
        if self.in_flight == 0 {
            self.processing_since = now_ms();
        }
        self.in_flight += 1;
        self.run_job(id, mode, audio_in, history_id, split);
        id
    }

    /// Processes a recording under a job id already counted in `in_flight`.
    fn run_job(&mut self, id: u64, mode: Mode, audio_in: AudioInput, history_id: Option<u64>, split: Option<Split>) {
        let s = self.shared.settings();
        if self.recording.is_none() {
            self.show(self.processing_state());
        }
        let provider = self.provider_for(&s);
        let audio_dir = self.shared.audio_dir();
        let tx = self.tx.clone();
        let opts = pipeline_options(&s);
        let created_at_ms = now_ms();
        let submitted = Instant::now();
        let supervisor_tx = tx.clone();
        let job = tokio::spawn(async move {
            let (wav, secs, audio_path) = match audio_in {
                AudioInput::Wav { bytes, secs } => (Some(bytes), secs, None),
                AudioInput::Captured(c) => {
                    let t_prep = Instant::now();
                    let prepared = tokio::task::spawn_blocking(move || audio::prepare(c)).await.ok().flatten();
                    tracing::info!(prepare_ms = t_prep.elapsed().as_millis() as u64, "timing: audio prepare");
                    match prepared {
                        Some(p) => {
                            // Job ids restart at 1 on every launch; the timestamp keeps names unique.
                            let path = audio_dir.join(format!("{created_at_ms}-{id}.wav"));
                            let saved = std::fs::create_dir_all(&audio_dir)
                                .and_then(|_| std::fs::write(&path, &p.wav))
                                .is_ok();
                            (Some(p.wav), p.audio_secs, saved.then_some(path))
                        }
                        None => (None, 0.0, None),
                    }
                }
            };
            let mut requests = 0;
            let mut outcome = None;
            let split = split.map(|sp| (sp.pieces, split_tail(sp.rest)));
            // A short recording sent nowhere yet gains nothing from the piecewise path.
            let split = split.filter(|(pieces, tail)| !pieces.is_empty() || tail.len() > 1);
            if let Some((pieces, tail)) = split {
                let crashed = || (Err(JobError::Provider(ProviderError::BadResponse("piece crashed".into()))), 1);
                let mut outs = Vec::new();
                for p in pieces {
                    outs.push(p.await.unwrap_or_else(|_| crashed()));
                }
                // The halves of a long tail run at the same time.
                let tail: Vec<_> = tail
                    .into_iter()
                    .map(|c| tokio::spawn(process_capture(mode, c, opts.clone(), provider.clone())))
                    .collect();
                for t in tail {
                    outs.push(t.await.unwrap_or_else(|_| crashed()));
                }
                requests = outs.iter().map(|(_, n)| n).sum();
                let outs: Vec<_> = outs.into_iter().map(|(o, _)| o).collect();
                if outs.iter().any(|o| matches!(o, Err(JobError::Provider(_)))) {
                    tracing::warn!("a piece failed; sending the whole recording in one request");
                } else {
                    outcome = Some(combine(outs));
                }
            }
            let outcome = match outcome {
                Some(o) => o,
                None => match wav {
                    None => Err(JobError::NothingHeard),
                    Some(wav) => {
                        let (o, n) = process(mode, wav, secs, &opts, provider).await;
                        requests += n;
                        o
                    }
                },
            };
            let requests = requests.max(1);
            JobResult { mode, submitted, history_id, audio_path, audio_secs: secs, created_at_ms, requests, outcome }
        });
        // A bug in one job must fail that job only, never stall the ordered queue.
        tokio::spawn(async move {
            let result = job.await.unwrap_or_else(|e| {
                tracing::error!("job {id} crashed: {e}");
                JobResult {
                    mode,
                    submitted,
                    history_id,
                    audio_path: None,
                    audio_secs: 0.0,
                    created_at_ms,
                    requests: 1,
                    outcome: Err(JobError::Provider(ProviderError::BadResponse("internal error".into()))),
                }
            });
            let _ = supervisor_tx.send(Input::JobDone(id, Box::new(result)));
        });
    }

    async fn finish(&mut self, r: JobResult, id: u64) {
        let s = self.shared.settings();
        let today = chrono::Local::now().date_naive();
        let stt_called = r.audio_secs > 0.0;
        if let Ok(mut u) = self.shared.usage.lock() {
            if stt_called && !matches!(r.outcome, Err(JobError::Provider(_))) {
                u.record_stt(today, r.audio_secs);
                for _ in 1..r.requests {
                    u.count_stt_request(today);
                }
            }
            if let Ok(out) = &r.outcome {
                if out.direct {
                    u.record_tokens(today, out.tokens);
                } else if out.llm_used && !out.degraded {
                    u.record_llm(today, out.tokens);
                }
            }
            let _ = u.save();
        }

        let trial = self.shared.trial.load(Ordering::SeqCst);
        let (status, text, raw, keep_audio, overlay, visible) = match r.outcome {
            Ok(out) if trial => {
                self.deps.ui.emit(UiEvent::Trial(TrialOutcome::Text { mode: r.mode, text: out.text.clone() }));
                let status = if out.degraded { EntryStatus::Degraded } else { EntryStatus::Done };
                let state = OverlayState::Done { outcome: "shown".into(), degraded: out.degraded };
                (status, out.text, out.raw, false, state, DONE_VISIBLE)
            }
            Ok(out) => {
                let text = out.text.clone();
                let method = s.insert_method;
                let output = self.deps.output.clone();
                let t_deliver = Instant::now();
                let delivered = tokio::task::spawn_blocking(move || output.deliver(&text, method))
                    .await
                    .unwrap_or(DeliveryOutcome::CopiedOnly(CopyReason::InputFailed));
                tracing::info!(
                    deliver_ms = t_deliver.elapsed().as_millis() as u64,
                    total_ms = r.submitted.elapsed().as_millis() as u64,
                    "timing: delivered (release → text inserted)"
                );
                let status = if out.degraded { EntryStatus::Degraded } else { EntryStatus::Done };
                let state = OverlayState::Done { outcome: outcome_name(delivered).into(), degraded: out.degraded };
                (status, out.text, out.raw, false, state, DONE_VISIBLE)
            }
            Err(JobError::NothingHeard) => (
                EntryStatus::NothingHeard,
                String::new(),
                String::new(),
                false,
                OverlayState::error("nothing_heard", None),
                ERROR_VISIBLE,
            ),
            Err(JobError::Provider(e)) => (
                {
                    tracing::warn!("job failed: {}", super::diagnostics::redact(&e.to_string()));
                    EntryStatus::Failed { message: e.to_string() }
                },
                String::new(),
                String::new(),
                true,
                error_state(&e, s.provider),
                ERROR_VISIBLE,
            ),
        };

        let new_entry_id = self.next_history_id();
        let mut to_delete = Vec::new();
        if !keep_audio {
            to_delete.extend(r.audio_path.clone());
        }
        if let Ok(mut h) = self.shared.history.lock() {
            match r.history_id {
                Some(hid) => to_delete.extend(h.update(hid, |e| {
                    e.status = status.clone();
                    if !text.is_empty() {
                        e.text = text.clone();
                        e.raw = raw.clone();
                    }
                })),
                None if s.history_enabled && (stt_called || keep_audio) => {
                    to_delete.extend(h.push(HistoryEntry {
                        id: new_entry_id,
                        created_at_ms: r.created_at_ms,
                        mode: r.mode,
                        text,
                        raw,
                        status,
                        audio_file: if keep_audio { r.audio_path.clone() } else { None },
                    }))
                }
                None => {
                    if keep_audio {
                        to_delete.extend(r.audio_path.clone());
                    }
                }
            }
            let _ = h.save();
        }
        remove_files(to_delete);
        self.deps.ui.emit(UiEvent::HistoryChanged);
        if trial {
            if let OverlayState::Error { message_key, .. } = &overlay {
                self.deps.ui.emit(UiEvent::Trial(TrialOutcome::Error { message_key: message_key.clone() }));
            }
        }
        self.show_result(overlay, visible);
        // The draft window leaves together with its own result (spec 2026-11-06 §3.1) — not when
        // an earlier job finishes while the next dictation shows its window.
        let seq = self.live_ids.remove(&id);
        let owner = self.draft_owner.load(Ordering::SeqCst);
        if owner == 0 || Some(owner) == seq {
            self.deps.overlay.draft(None);
            self.draft_owner.store(0, Ordering::SeqCst);
        }
    }

    /// History ids are unique across restarts: milliseconds since epoch, bumped on collision.
    fn next_history_id(&self) -> u64 {
        let mut id = now_ms().max(0) as u64;
        if let Ok(h) = self.shared.history.lock() {
            while h.get(id).is_some() {
                id += 1;
            }
        }
        id
    }
}

/// One request over the backup routes for the verbatim text; failing that, Dictation pastes the
/// verbatim text and Translate takes the batch path. Returns the outcome and the requests made.
async fn finish_text_work(
    mode: Mode,
    captured: Arc<Captured>,
    raw: String,
    requests: u32,
    system: Option<String>,
    opts: PipelineOptions,
    backend: Option<Backend>,
) -> (Result<JobOutput, JobError>, u32) {
    let models: Vec<Arc<dyn Provider>> = match &backend {
        Some(b) if !b.routes.is_empty() => b.routes.iter().map(|(_, p)| p.clone()).collect(),
        Some(b) => vec![b.single.clone()],
        None => Vec::new(),
    };
    let mut n = requests;
    let mut text = None;
    if let Some(system) = system {
        let attempts: Vec<routes::Attempt<String>> = models
            .into_iter()
            .map(|p| {
                let (system, user) = (system.clone(), pipeline::user_message(&raw));
                Box::pin(async move { p.complete(&system, &user).await.map(|c| pipeline::postprocess(&c.text)) }) as routes::Attempt<String>
            })
            .collect();
        let (r, reports) = routes::run_hedged(attempts, routes::hedge_after(0.0)).await;
        n += reports.len() as u32;
        text = r.ok().filter(|t| !t.trim().is_empty());
    }
    let outcome = match text {
        Some(text) => Ok(JobOutput { text, raw, degraded: false, tokens: 0, llm_used: true, direct: false }),
        None if mode == Mode::Dictation => Ok(JobOutput { text: raw.clone(), raw, degraded: true, tokens: 0, llm_used: true, direct: false }),
        None => {
            let (o, m) = process_capture(mode, (*captured).clone(), opts, backend).await;
            n += m;
            o
        }
    };
    (outcome, n)
}

/// The recording as a 16 kHz wav in the audio folder (for Retry); `None` if it has no speech.
async fn save_audio(dir: &std::path::Path, created_at_ms: i64, id: u64, captured: &Arc<Captured>) -> Option<PathBuf> {
    let c = (**captured).clone();
    let prepared = tokio::task::spawn_blocking(move || audio::prepare(c)).await.ok().flatten()?;
    let path = dir.join(format!("{created_at_ms}-{id}.wav"));
    std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&path, &prepared.wav)).ok()?;
    Some(path)
}

/// The prompt that finishes a live dictation's pieces: translation always, cleanup when on.
fn live_system(mode: Mode, opts: &PipelineOptions) -> Option<String> {
    pipeline::system_prompt(mode, opts, 0.0)
}

fn pipeline_options(s: &Settings) -> PipelineOptions {
    PipelineOptions {
        language: s.speech_language.clone(),
        cleanup: s.cleanup,
        paragraphs: s.paragraphs,
        style: s.translate_style,
    }
}

/// One recording through the provider: its backup routes when it has them and the job
/// needs an LLM step, else one request (plus the LLM step when the provider needs a
/// second one). Returns the result and the speech requests made.
async fn process(
    mode: Mode,
    wav: Vec<u8>,
    secs: f32,
    opts: &PipelineOptions,
    backend: Option<Backend>,
) -> (Result<JobOutput, JobError>, u32) {
    let Some(b) = backend else { return (Err(JobError::Provider(ProviderError::Unauthorized)), 1) };
    // Without an LLM step every route would be the same transcription model.
    if b.routes.is_empty() || pipeline::system_prompt(mode, opts, secs).is_none() {
        let p = b.single.as_ref();
        let out = match p.direct() {
            Some(direct) if pipeline::audio_system_prompt(mode, opts, secs).is_some() => {
                pipeline::run_direct(mode, wav, secs, opts, direct, p).await
            }
            _ => pipeline::run(mode, wav, secs, opts, p, p).await,
        };
        return (out, 1);
    }
    process_routed(mode, wav, secs, opts, &b).await
}

/// Backup models with hedging (spec 2026-11-04 §4.5): the next route starts when the
/// current one is slow or fails; the first answer wins.
async fn process_routed(
    mode: Mode,
    wav: Vec<u8>,
    secs: f32,
    opts: &PipelineOptions,
    b: &Backend,
) -> (Result<JobOutput, JobError>, u32) {
    let all: Vec<usize> = (0..b.routes.len()).collect();
    let order: Vec<usize> = match b.health.lock() {
        Ok(h) => h.order(Instant::now()).into_iter().filter(|&i| i < b.routes.len()).collect(),
        Err(_) => all,
    };
    let attempts: Vec<routes::Attempt<Result<JobOutput, JobError>>> = order
        .iter()
        .map(|&i| {
            let (route, p) = b.routes[i].clone();
            let (wav, opts) = (wav.clone(), opts.clone());
            Box::pin(async move {
                let r = match (route, p.direct()) {
                    (Route::Direct { .. }, Some(audio)) => pipeline::run_audio_only(mode, wav, secs, &opts, audio).await,
                    _ => pipeline::run(mode, wav, secs, &opts, p.as_ref(), p.as_ref()).await,
                };
                // "Nothing heard" is an answer; only provider errors move on to the next route.
                match r {
                    Err(JobError::Provider(e)) => Err(e),
                    other => Ok(other),
                }
            }) as routes::Attempt<_>
        })
        .collect();
    let t = Instant::now();
    let (result, reports) = routes::run_hedged(attempts, routes::hedge_after(secs)).await;
    let now = Instant::now();
    if let Ok(mut h) = b.health.lock() {
        for r in &reports {
            let bad = match &r.outcome {
                AttemptOutcome::Slow => true,
                AttemptOutcome::Failed(e) => *e != ProviderError::Unauthorized,
                AttemptOutcome::Won | AttemptOutcome::Cancelled => false,
            };
            if bad {
                h.punish(order[r.index], now);
            }
        }
    }
    let won = reports.iter().find(|r| r.outcome == AttemptOutcome::Won).map_or(-1, |r| order[r.index] as i64);
    let detail: Vec<String> = reports
        .iter()
        .map(|r| {
            let what = match &r.outcome {
                AttemptOutcome::Won => "won".to_string(),
                AttemptOutcome::Slow => "slow".to_string(),
                AttemptOutcome::Cancelled => "cancelled".to_string(),
                AttemptOutcome::Failed(e) => format!("failed ({})", super::diagnostics::redact(&e.to_string())),
            };
            format!("{}: {what}", order[r.index])
        })
        .collect();
    tracing::info!(won, attempts = reports.len(), ms = t.elapsed().as_millis() as u64, detail = detail.join(", "), "route won");
    let out = match result {
        Ok(r) => r,
        Err(e) => Err(JobError::Provider(e)),
    };
    (out, reports.len().max(1) as u32)
}

/// Prepares raw audio off the async threads, then processes it.
async fn process_capture(
    mode: Mode,
    c: Captured,
    opts: PipelineOptions,
    provider: Option<Backend>,
) -> (Result<JobOutput, JobError>, u32) {
    let prepared = tokio::task::spawn_blocking(move || audio::prepare(c)).await.ok().flatten();
    match prepared {
        Some(p) => process(mode, p.wav, p.audio_secs, &opts, provider).await,
        None => (Err(JobError::NothingHeard), 0),
    }
}

/// The audio left at release: halved at its quietest middle moment when long.
fn split_tail(rest: Captured) -> Vec<Captured> {
    let ch = rest.channels.max(1) as usize;
    let mono = dsp::downmix(&rest.samples, rest.channels);
    match segment::split_point(&mono, rest.rate) {
        Some(cut) => {
            let n = (cut * ch).min(rest.samples.len());
            let second = Captured { samples: rest.samples[n..].to_vec(), ..rest.clone() };
            let first = Captured { samples: rest.samples[..n].to_vec(), ..rest };
            vec![first, second]
        }
        None => vec![rest],
    }
}

/// Joins the pieces of one recording in order; silent pieces are skipped.
fn combine(outs: Vec<Result<JobOutput, JobError>>) -> Result<JobOutput, JobError> {
    let parts: Vec<JobOutput> = outs.into_iter().filter_map(Result::ok).filter(|o| !o.text.trim().is_empty()).collect();
    if parts.is_empty() {
        return Err(JobError::NothingHeard);
    }
    let join = |f: fn(&JobOutput) -> &str| parts.iter().map(|o| f(o).trim()).collect::<Vec<_>>().join(" ");
    Ok(JobOutput {
        text: join(|o| &o.text),
        raw: join(|o| &o.raw),
        degraded: parts.iter().any(|o| o.degraded),
        tokens: parts.iter().map(|o| o.tokens).sum(),
        llm_used: parts.iter().any(|o| o.llm_used),
        direct: parts.iter().all(|o| o.direct),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Captured;
    use crate::controller::HotkeyEvent;
    use crate::output::DeliveryOutcome;
    use crate::permissions::PermStatus;
    use crate::providers::{Completion, ProviderError, SpeechToText, TextModel};
    use crate::recorder::{AudioSource, RecorderError};
    use crate::secrets::{MemoryStore, SecretStore};
    use crate::settings::{InsertMethod, ProviderKind, Settings};
        use crate::types::Mode;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    fn speech(secs: f32) -> Captured {
        let rate = 16_000;
        let n = (rate as f32 * secs) as usize;
        let samples = (0..n)
            .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / rate as f32).sin())
            .collect();
        Captured { samples, rate, channels: 1 }
    }

    #[derive(Default)]
    struct FakeAudio {
        captures: Mutex<VecDeque<Captured>>,
        fail_start: Mutex<Option<RecorderError>>,
        starts: AtomicUsize,
        /// What the microphone has recorded so far, for processing while speaking.
        live: Mutex<Option<Captured>>,
    }
    impl AudioSource for FakeAudio {
        fn start(&self) -> Result<(), RecorderError> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            match self.fail_start.lock().unwrap().take() {
                Some(e) => Err(e),
                None => Ok(()),
            }
        }
        fn stop(&self) -> Result<Captured, RecorderError> {
            self.captures.lock().unwrap().pop_front().ok_or(RecorderError::NotRecording)
        }
        fn cancel(&self) {}
        fn peek(&self, from: usize) -> Option<Captured> {
            let live = self.live.lock().unwrap().clone()?;
            Some(Captured { samples: live.samples.get(from..)?.to_vec(), ..live })
        }
    }

    /// Long audio (> 1.5 s) is answered slowly with `slow`, short audio quickly with `fast`.
    struct FakeProvider {
        slow: Result<String, ProviderError>,
        fast: Result<String, ProviderError>,
        stt_calls: AtomicUsize,
        /// When set, the provider offers a one-request audio→text path returning this.
        direct_reply: Option<String>,
        /// Long (slow) audio makes transcription panic.
        panic_on_slow: bool,
        direct_calls: AtomicUsize,
        /// Replies for successive one-request calls; `direct_reply` once these run out.
        direct_replies: Mutex<VecDeque<Result<String, ProviderError>>>,
        /// How long a one-request call takes.
        direct_delay: Duration,
        /// The text model answers with a server error.
        text_fail: bool,
        /// The text model panics (a bug).
        text_panic: bool,
        /// The text model's answer ("Привет." when unset) and how long it takes.
        text_reply: Option<String>,
        text_delay: Duration,
    }
    impl FakeProvider {
        fn new(slow: Result<String, ProviderError>, fast: Result<String, ProviderError>) -> Self {
            Self {
                slow,
                fast,
                stt_calls: AtomicUsize::new(0),
                direct_reply: None,
                panic_on_slow: false,
                direct_calls: AtomicUsize::new(0),
                direct_replies: Mutex::new(VecDeque::new()),
                direct_delay: Duration::ZERO,
                text_fail: false,
                text_panic: false,
                text_reply: None,
                text_delay: Duration::ZERO,
            }
        }
    }
    #[async_trait::async_trait]
    impl AudioLlm for FakeProvider {
        async fn complete_audio(&self, _: &str, _: Vec<u8>, _: Option<String>, _: f32) -> Result<Completion, ProviderError> {
            self.direct_calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(self.direct_delay).await;
            if let Some(r) = self.direct_replies.lock().unwrap().pop_front() {
                return r.map(|text| Completion { text, total_tokens: 5 });
            }
            Ok(Completion { text: self.direct_reply.clone().unwrap_or_default(), total_tokens: 5 })
        }
    }
    #[async_trait::async_trait]
    impl SpeechToText for FakeProvider {
        async fn transcribe(&self, _: Vec<u8>, _: Option<String>, secs: f32) -> Result<String, ProviderError> {
            self.stt_calls.fetch_add(1, Ordering::SeqCst);
            if secs > 1.5 {
                assert!(!self.panic_on_slow, "simulated bug in a job");
                tokio::time::sleep(Duration::from_millis(300)).await;
                self.slow.clone()
            } else {
                tokio::time::sleep(Duration::from_millis(10)).await;
                self.fast.clone()
            }
        }
    }
    #[async_trait::async_trait]
    impl TextModel for FakeProvider {
        async fn complete(&self, _: &str, _: &str) -> Result<Completion, ProviderError> {
            assert!(!self.text_panic, "simulated bug in the text model");
            tokio::time::sleep(self.text_delay).await;
            if self.text_fail {
                return Err(ProviderError::Server(500));
            }
            Ok(Completion { text: self.text_reply.clone().unwrap_or_else(|| "Привет.".into()), total_tokens: 3 })
        }
    }
    #[async_trait::async_trait]
    impl Provider for FakeProvider {
        async fn prewarm(&self) {}
        async fn verify_key(&self) -> Result<(), ProviderError> {
            Ok(())
        }
        fn direct(&self) -> Option<&dyn AudioLlm> {
            self.direct_reply.as_ref().map(|_| self as &dyn AudioLlm)
        }
    }
    struct FakeFactory(Arc<FakeProvider>);
    impl ProviderFactory for FakeFactory {
        fn make(&self, _: ProviderKind, _: String) -> Arc<dyn Provider> {
            self.0.clone()
        }
    }

    /// A separate fake per backup route.
    struct RouteFactory {
        single: Arc<FakeProvider>,
        routes: Vec<Arc<FakeProvider>>,
        made: Mutex<Vec<Route>>,
    }
    impl RouteFactory {
        /// One-request fakes answering `reply` after `delay`.
        fn new(routes: Vec<(Duration, &str)>) -> Arc<Self> {
            let fake = |delay: Duration, reply: &str| {
                let mut p = FakeProvider::new(Ok("x".into()), Ok("сырой".into()));
                p.direct_reply = Some(reply.into());
                p.direct_delay = delay;
                Arc::new(p)
            };
            Arc::new(Self {
                single: fake(Duration::ZERO, "Один маршрут."),
                routes: routes.into_iter().map(|(d, r)| fake(d, r)).collect(),
                made: Mutex::new(Vec::new()),
            })
        }
        /// Text models (cleanup, translation) answering `reply` after `delay`, one per route.
        fn texts(routes: Vec<(Duration, &str)>) -> Arc<Self> {
            let fake = |delay: Duration, reply: &str| {
                let mut p = FakeProvider::new(Ok("x".into()), Ok("сырой".into()));
                p.text_reply = Some(reply.into());
                p.text_delay = delay;
                Arc::new(p)
            };
            Arc::new(Self {
                single: fake(Duration::ZERO, "Один маршрут."),
                routes: routes.into_iter().map(|(d, r)| fake(d, r)).collect(),
                made: Mutex::new(Vec::new()),
            })
        }
        fn calls(&self) -> Vec<usize> {
            self.routes.iter().map(|p| p.direct_calls.load(Ordering::SeqCst)).collect()
        }
    }
    impl ProviderFactory for RouteFactory {
        fn make(&self, _: ProviderKind, _: String) -> Arc<dyn Provider> {
            self.single.clone()
        }
        fn make_route(&self, _: ProviderKind, _: String, route: Route) -> Arc<dyn Provider> {
            let mut made = self.made.lock().unwrap();
            let i = defaults::GEMINI_ROUTES.iter().position(|r| *r == route).unwrap();
            made.push(route);
            self.routes[i].clone()
        }
    }

    #[derive(Default)]
    struct FakeOutput(Mutex<Vec<String>>);
    impl Deliverer for FakeOutput {
        fn deliver(&self, text: &str, _: InsertMethod) -> DeliveryOutcome {
            self.0.lock().unwrap().push(text.to_string());
            DeliveryOutcome::Pasted
        }
    }

    #[derive(Default)]
    struct FakeOverlay(Mutex<Vec<OverlayState>>, Mutex<Vec<Option<crate::live::draft::DraftParts>>>);
    impl OverlaySink for FakeOverlay {
        fn show(&self, s: &OverlayState) {
            self.0.lock().unwrap().push(s.clone());
        }
        fn draft(&self, parts: Option<&crate::live::draft::DraftParts>) {
            self.1.lock().unwrap().push(parts.cloned());
        }
    }
    impl FakeOverlay {
        fn kinds(&self) -> Vec<String> {
            self.0.lock().unwrap().iter().map(|s| s.kind().to_string()).collect()
        }
        fn last_error(&self) -> Option<String> {
            self.0.lock().unwrap().iter().rev().find_map(|s| match s {
                OverlayState::Error { message_key, .. } => Some(message_key.clone()),
                _ => None,
            })
        }
    }

    #[derive(Default)]
    struct FakeUi(Mutex<Vec<UiEvent>>);
    impl UiSink for FakeUi {
        fn emit(&self, e: UiEvent) {
            self.0.lock().unwrap().push(e);
        }
    }
    impl FakeUi {
        fn statuses(&self) -> Vec<AppStatus> {
            self.0.lock().unwrap().iter().filter_map(|e| match e { UiEvent::Status(s) => Some(*s), _ => None }).collect()
        }
    }

    struct AllGranted(Mutex<PermStatus>);
    impl PermissionCheck for AllGranted {
        fn accessibility(&self) -> bool {
            true
        }
        fn microphone(&self) -> PermStatus {
            *self.0.lock().unwrap()
        }
    }

    #[derive(Default)]
    struct NoHotkeys {
        recording: std::sync::atomic::AtomicBool,
        restarts: AtomicUsize,
    }
    impl HotkeyControl for NoHotkeys {
        fn set_bindings(&self, _: crate::settings::Hotkeys) {}
        fn set_recording(&self, on: bool) {
            self.recording.store(on, Ordering::SeqCst);
        }
        fn ensure_running(&self) -> bool {
            true
        }
        fn restart(&self) {
            self.restarts.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct Rig {
        orch: Orchestrator,
        rx: tokio::sync::mpsc::UnboundedReceiver<Input>,
        audio: Arc<FakeAudio>,
        provider: Arc<FakeProvider>,
        out: Arc<FakeOutput>,
        overlay: Arc<FakeOverlay>,
        shared: Arc<Shared>,
        hotkeys: Arc<NoHotkeys>,
        perms: Arc<AllGranted>,
        ui: Arc<FakeUi>,
        secrets: Arc<MemoryStore>,
        _dir: tempfile::TempDir,
    }

    fn rig(provider: FakeProvider, with_key: bool) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = Settings::defaults_for(Some("ru"));
        settings.cleanup = false;
        let shared = Arc::new(Shared::new(settings, dir.path().to_path_buf()));
        let secrets = Arc::new(MemoryStore::default());
        if with_key {
            let provider = shared.settings().provider;
            secrets.set(provider, "test-key").unwrap();
        }
        let audio = Arc::new(FakeAudio::default());
        let provider = Arc::new(provider);
        let out = Arc::new(FakeOutput::default());
        let overlay = Arc::new(FakeOverlay::default());
        let hotkeys = Arc::new(NoHotkeys::default());
        let perms = Arc::new(AllGranted(Mutex::new(PermStatus::Granted)));
        let ui = Arc::new(FakeUi::default());
        let deps = Deps {
            secrets: secrets.clone(),
            audio: audio.clone(),
            providers: Arc::new(FakeFactory(provider.clone())),
            output: out.clone(),
            overlay: overlay.clone(),
            permissions: perms.clone(),
            hotkeys: hotkeys.clone(),
            ui: ui.clone(),
            live: None,
        };
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let orch = Orchestrator::new(deps, shared.clone(), tx);
        Rig { orch, rx, audio, provider, out, overlay, shared, hotkeys, perms, ui, secrets, _dir: dir }
    }

    impl Rig {
        async fn record(&mut self, mode: Mode, capture: Captured, t0: Instant, secs: u64) {
            self.audio.captures.lock().unwrap().push_back(capture);
            self.orch.handle(Input::Hotkey(HotkeyEvent::Down(mode)), t0).await;
            self.orch.handle(Input::Hotkey(HotkeyEvent::Up(mode)), t0 + Duration::from_secs(secs)).await;
        }

        /// Feeds job results back until `n` deliveries/outcomes happened or 3 s pass.
        async fn settle(&mut self, n: usize) {
            let mut done = 0;
            while done < n {
                let input = tokio::time::timeout(Duration::from_secs(3), self.rx.recv())
                    .await
                    .expect("timed out waiting for job")
                    .expect("channel closed");
                if matches!(input, Input::JobDone(..)) {
                    done += 1;
                }
                self.orch.handle(input, Instant::now()).await;
            }
        }
    }

    #[tokio::test]
    async fn dictation_end_to_end_pastes() {
        let mut r = rig(FakeProvider::new(Ok("unused".into()), Ok("Привет, мир.".into())), true);
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Привет, мир.".to_string()]);
        assert_eq!(r.overlay.kinds()[..3], ["recording", "processing", "done"]);
        let h = r.shared.history.lock().unwrap();
        assert_eq!(h.entries().front().map(|e| e.text.as_str()), Some("Привет, мир."));
        assert_eq!(h.entries().front().and_then(|e| e.audio_file.clone()), None);
    }

    #[tokio::test]
    async fn cleanup_runs_llm_when_enabled() {
        let mut r = rig(FakeProvider::new(Ok("unused".into()), Ok("привет".into())), true);
        r.shared.settings.lock().unwrap().cleanup = true;
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Привет.".to_string()]);
        let usage = r.shared.usage.lock().unwrap().today(chrono::Local::now().date_naive());
        assert_eq!((usage.stt_requests, usage.llm_requests, usage.llm_tokens), (1, 1, 3));
    }

    #[tokio::test]
    async fn missing_key_shows_error_and_stays_idle() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Ok("x".into())), false);
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Dictation)), Instant::now()).await;
        assert_eq!(r.overlay.last_error().as_deref(), Some("no_key"));
        assert_eq!(r.audio.starts.load(Ordering::SeqCst), 0);
        assert!(matches!(r.orch.controller_state(), crate::controller::State::Idle));
    }

    #[tokio::test]
    async fn silence_shows_nothing_heard_without_request() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Ok("x".into())), true);
        let silence = Captured { samples: vec![0.0; 16_000], rate: 16_000, channels: 1 };
        r.record(Mode::Dictation, silence, Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(r.overlay.last_error().as_deref(), Some("nothing_heard"));
        assert_eq!(r.provider.stt_calls.load(Ordering::SeqCst), 0);
        assert!(r.out.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn recorder_failure_aborts_controller() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Ok("x".into())), true);
        *r.audio.fail_start.lock().unwrap() = Some(RecorderError::NoDevice);
        let t0 = Instant::now();
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Dictation)), t0).await;
        assert_eq!(r.overlay.last_error().as_deref(), Some("no_microphone"));
        assert!(matches!(r.orch.controller_state(), crate::controller::State::Idle));
        r.orch.handle(Input::Hotkey(HotkeyEvent::Up(Mode::Dictation)), t0).await;
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Dictation)), t0).await;
        assert_eq!(r.audio.starts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn results_delivered_in_recording_order() {
        let mut r = rig(FakeProvider::new(Ok("one".into()), Ok("two".into())), true);
        let t0 = Instant::now();
        r.record(Mode::Dictation, speech(2.0), t0, 2).await;
        r.record(Mode::Dictation, speech(1.0), t0 + Duration::from_secs(3), 1).await;
        r.settle(2).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["one".to_string(), "two".to_string()]);
    }

    #[tokio::test]
    async fn rate_limit_error_keeps_audio_for_retry() {
        let limited = Err(ProviderError::RateLimited { retry_after: Some(Duration::from_secs(600)) });
        let mut r = rig(FakeProvider::new(Ok("x".into()), limited), true);
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(r.overlay.last_error().as_deref(), Some("rate_limited"));
        let audio = {
            let h = r.shared.history.lock().unwrap();
            let e = h.entries().front().unwrap().clone();
            assert!(matches!(e.status, crate::history::EntryStatus::Failed { .. }));
            e.audio_file.expect("audio kept for retry")
        };
        assert!(audio.exists());
        assert!(r.out.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn retry_redelivers_and_releases_audio() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Err(ProviderError::Network)), true);
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        let (id, audio) = {
            let h = r.shared.history.lock().unwrap();
            let e = h.entries().front().unwrap();
            (e.id, e.audio_file.clone().unwrap())
        };
        // Network is back.
        let fixed = Arc::new(FakeProvider::new(Ok("x".into()), Ok("снова".into())));
        r.orch.set_provider_factory(Arc::new(FakeFactory(fixed)));
        r.orch.handle(Input::Retry(id), Instant::now()).await;
        r.settle(1).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["снова".to_string()]);
        assert!(!audio.exists());
        let h = r.shared.history.lock().unwrap();
        assert_eq!(h.get(id).map(|e| e.text.as_str()), Some("снова"));
    }

    #[tokio::test]
    async fn direct_provider_makes_one_request() {
        let mut p = FakeProvider::new(Ok("x".into()), Ok("raw".into()));
        p.direct_reply = Some("Hello there.".into());
        let mut r = rig(p, true);
        r.shared.settings.lock().unwrap().cleanup = true;
        r.record(Mode::Translate, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Hello there.".to_string()]);
        assert_eq!(r.provider.direct_calls.load(Ordering::SeqCst), 1);
        assert_eq!(r.provider.stt_calls.load(Ordering::SeqCst), 0);
    }

    fn stt_requests_today(r: &Rig) -> u32 {
        r.shared.usage.lock().unwrap().today(chrono::Local::now().date_naive()).stt_requests
    }

    #[tokio::test(start_paused = true)]
    async fn slow_primary_still_delivers() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Ok("x".into())), true);
        r.shared.settings.lock().unwrap().cleanup = true;
        let f = RouteFactory::new(vec![
            (Duration::from_secs(20), "Медленно."),
            (Duration::from_millis(100), "Быстро."),
            (Duration::ZERO, "Третий."),
        ]);
        r.orch.set_provider_factory(f.clone());
        let t = tokio::time::Instant::now();
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Быстро.".to_string()]);
        assert!(t.elapsed() < Duration::from_secs(4), "{:?}", t.elapsed());
        assert_eq!(f.calls()[0], 1);
        assert_eq!(f.routes[2].direct_calls.load(Ordering::SeqCst), 0);
        assert_eq!(stt_requests_today(&r), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn slow_route_is_skipped_next_time() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Ok("x".into())), true);
        r.shared.settings.lock().unwrap().cleanup = true;
        let f = RouteFactory::new(vec![
            (Duration::from_secs(20), "Медленно."),
            (Duration::ZERO, "Быстро."),
            (Duration::ZERO, "Третий."),
        ]);
        r.orch.set_provider_factory(f.clone());
        r.record(Mode::Translate, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        let t = tokio::time::Instant::now();
        r.record(Mode::Translate, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
        assert_eq!(f.calls()[0], 1, "the slow route was not tried again");
        assert_eq!(r.out.0.lock().unwrap().len(), 2);
        assert_eq!(stt_requests_today(&r), 3);
    }

    #[tokio::test]
    async fn groq_uses_single_route() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Ok("x".into())), true);
        {
            let mut s = r.shared.settings.lock().unwrap();
            s.cleanup = true;
            s.provider = ProviderKind::Groq;
        }
        r.secrets.set(ProviderKind::Groq, "groq-key").unwrap();
        let f = RouteFactory::new(vec![(Duration::ZERO, "a"), (Duration::ZERO, "b"), (Duration::ZERO, "c")]);
        r.orch.set_provider_factory(f.clone());
        r.record(Mode::Translate, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Один маршрут.".to_string()]);
        assert!(f.made.lock().unwrap().is_empty());
        assert_eq!(f.calls(), vec![0, 0, 0]);
    }

    #[tokio::test]
    async fn recording_flag_follows_recording() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Ok("x".into())), true);
        let t0 = Instant::now();
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Dictation)), t0).await;
        assert!(r.hotkeys.recording.load(Ordering::SeqCst));
        r.orch.handle(Input::Hotkey(HotkeyEvent::Escape), t0 + Duration::from_secs(1)).await;
        assert!(!r.hotkeys.recording.load(Ordering::SeqCst));
        r.audio.captures.lock().unwrap().push_back(speech(1.0));
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Dictation)), t0 + Duration::from_secs(2)).await;
        r.orch.handle(Input::Hotkey(HotkeyEvent::Up(Mode::Dictation)), t0 + Duration::from_secs(3)).await;
        assert!(!r.hotkeys.recording.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn error_during_recording_is_shown_after_recording_ends() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Err(ProviderError::Network)), true);
        let t0 = Instant::now();
        r.record(Mode::Dictation, speech(1.0), t0, 1).await;
        // A new recording starts before the first job fails.
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Dictation)), t0 + Duration::from_secs(2)).await;
        r.settle(1).await;
        assert_eq!(r.overlay.last_error(), None, "no error over an active recording");
        r.orch.handle(Input::Hotkey(HotkeyEvent::Escape), t0 + Duration::from_secs(4)).await;
        assert_eq!(r.overlay.last_error().as_deref(), Some("network"));
    }

    #[tokio::test]
    async fn wake_gap_restarts_hotkeys_even_though_instant_pauses_in_sleep() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Ok("x".into())), true);
        let wall = Arc::new(Mutex::new(std::time::SystemTime::now()));
        let w = wall.clone();
        r.orch.set_wall_clock(Box::new(move || *w.lock().unwrap()));
        let t0 = Instant::now();
        r.orch.handle(Input::Tick, t0).await;
        // The machine sleeps for an hour: Instant (uptime) barely moves, wall time does.
        *wall.lock().unwrap() += Duration::from_secs(3600);
        r.orch.handle(Input::Tick, t0 + Duration::from_millis(100)).await;
        assert_eq!(r.hotkeys.restarts.load(Ordering::SeqCst), 1);
    }

    /// Counts fresh connection pools (review 2026-11-07: the shared Gemini pool outlived sleep).
    struct ResetFactory {
        inner: FakeFactory,
        resets: AtomicUsize,
    }
    impl ProviderFactory for ResetFactory {
        fn make(&self, kind: ProviderKind, key: String) -> Arc<dyn Provider> {
            self.inner.make(kind, key)
        }
        fn reset(&self) {
            self.resets.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn wake_gap_opens_fresh_connections() {
        // After sleep (VPN or network change) an old connection can be silently dead: every
        // route would hang on it until the 10 s timeout.
        let mut r = rig(FakeProvider::new(Ok("x".into()), Ok("x".into())), true);
        let f = Arc::new(ResetFactory { inner: FakeFactory(r.provider.clone()), resets: AtomicUsize::new(0) });
        r.orch.set_provider_factory(f.clone());
        let wall = Arc::new(Mutex::new(std::time::SystemTime::now()));
        let w = wall.clone();
        r.orch.set_wall_clock(Box::new(move || *w.lock().unwrap()));
        let t0 = Instant::now();
        r.orch.handle(Input::Tick, t0).await;
        assert_eq!(f.resets.load(Ordering::SeqCst), 0);
        *wall.lock().unwrap() += Duration::from_secs(3600);
        r.orch.handle(Input::Tick, t0 + Duration::from_millis(100)).await;
        assert_eq!(f.resets.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_audio_survives_restart() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Err(ProviderError::Network)), true);
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        let old = r.shared.history.lock().unwrap().entries().front().unwrap().audio_file.clone().unwrap();
        // Restart: a fresh orchestrator over the same data directory, next job succeeds.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let deps = Deps {
            secrets: {
                let s = Arc::new(MemoryStore::default());
                s.set(r.shared.settings().provider, "k").unwrap();
                s
            },
            audio: r.audio.clone(),
            providers: Arc::new(FakeFactory(Arc::new(FakeProvider::new(Ok("x".into()), Ok("new".into()))))),
            output: r.out.clone(),
            overlay: r.overlay.clone(),
            permissions: r.perms.clone(),
            hotkeys: r.hotkeys.clone(),
            ui: r.ui.clone(),
            live: None,
        };
        r.orch = Orchestrator::new(deps, r.shared.clone(), tx);
        r.rx = rx;
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert!(old.exists(), "the failed entry's audio must not be overwritten or deleted");
    }

    #[tokio::test]
    async fn denied_microphone_shows_error_without_recording() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Ok("x".into())), true);
        *r.perms.0.lock().unwrap() = PermStatus::Denied;
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Dictation)), Instant::now()).await;
        assert_eq!(r.overlay.last_error().as_deref(), Some("no_microphone_access"));
        assert_eq!(r.audio.starts.load(Ordering::SeqCst), 0);
        assert!(matches!(r.orch.controller_state(), crate::controller::State::Idle));
    }

    #[tokio::test]
    async fn panicking_job_still_releases_queue() {
        let mut p = FakeProvider::new(Ok("never".into()), Ok("two".into()));
        p.panic_on_slow = true;
        let mut r = rig(p, true);
        let t0 = Instant::now();
        r.record(Mode::Dictation, speech(2.0), t0, 2).await;
        r.record(Mode::Dictation, speech(1.0), t0 + Duration::from_secs(3), 1).await;
        r.settle(2).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["two".to_string()]);
    }

    #[tokio::test]
    async fn status_events_follow_recording_and_processing() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Ok("Привет.".into())), true);
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(r.ui.statuses(), vec![AppStatus::Recording, AppStatus::Processing, AppStatus::Idle]);
    }

    #[tokio::test]
    async fn hotkey_presses_are_forwarded_to_ui() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Ok("x".into())), true);
        r.record(Mode::Translate, speech(1.0), Instant::now(), 1).await;
        let keys: Vec<UiEvent> = r.ui.0.lock().unwrap().iter().filter(|e| matches!(e, UiEvent::Hotkey { .. })).cloned().collect();
        assert_eq!(
            keys,
            vec![
                UiEvent::Hotkey { mode: Mode::Translate, down: true },
                UiEvent::Hotkey { mode: Mode::Translate, down: false },
            ]
        );
    }

    #[tokio::test]
    async fn escape_hides_with_cancelled_state() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Ok("x".into())), true);
        let t0 = Instant::now();
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Dictation)), t0).await;
        r.orch.handle(Input::Hotkey(HotkeyEvent::Escape), t0 + Duration::from_secs(1)).await;
        assert_eq!(r.overlay.0.lock().unwrap().last(), Some(&OverlayState::Cancelled));
    }

    #[tokio::test]
    async fn error_hide_returns_to_processing_while_jobs_remain() {
        // The short recording fails fast; the long one is still processing.
        let mut r = rig(FakeProvider::new(Ok("late".into()), Err(ProviderError::Network)), true);
        let t0 = Instant::now();
        r.record(Mode::Dictation, speech(1.0), t0, 1).await;
        r.record(Mode::Dictation, speech(2.0), t0 + Duration::from_secs(2), 2).await;
        r.settle(1).await;
        assert_eq!(r.overlay.last_error().as_deref(), Some("network"));
        let seq = r.orch.overlay_seq;
        r.orch.handle(Input::HideOverlay(seq), Instant::now()).await;
        assert_eq!(r.overlay.kinds().last().map(String::as_str), Some("processing"));
    }

    #[tokio::test]
    async fn trial_mode_shows_result_in_window_without_pasting() {
        let mut r = rig(FakeProvider::new(Ok("x".into()), Ok("Проба.".into())), true);
        r.shared.trial.store(true, Ordering::SeqCst);
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert!(r.out.0.lock().unwrap().is_empty());
        let events = r.ui.0.lock().unwrap().clone();
        assert!(events.contains(&UiEvent::Trial(TrialOutcome::Text { mode: Mode::Dictation, text: "Проба.".into() })));
        assert!(events.contains(&UiEvent::HistoryChanged));
        assert!(r.overlay.0.lock().unwrap().contains(&OverlayState::Done { outcome: "shown".into(), degraded: false }));
    }

    #[tokio::test]
    async fn direct_request_counts_once() {
        let mut p = FakeProvider::new(Ok("x".into()), Ok("raw".into()));
        p.direct_reply = Some("Hello.".into());
        let mut r = rig(p, true);
        r.shared.settings.lock().unwrap().cleanup = true;
        r.record(Mode::Translate, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        let u = r.shared.usage.lock().unwrap().today(chrono::Local::now().date_naive());
        assert_eq!((u.stt_requests, u.llm_requests, u.llm_tokens), (1, 0, 5));
    }

    #[tokio::test]
    async fn rate_limit_error_names_provider() {
        let limited = Err(ProviderError::RateLimited { retry_after: None });
        let mut r = rig(FakeProvider::new(Ok("x".into()), limited), true);
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        let last = r.overlay.0.lock().unwrap().last().cloned();
        assert!(matches!(last, Some(OverlayState::Error { provider: Some(ref p), .. }) if p == "Gemini"), "{last:?}");
    }

    /// 12 s of speech, a 0.6 s pause, then 4 s more: one cut while speaking.
    fn long_speech() -> Captured {
        let mut c = speech(12.0);
        c.samples.extend(vec![0.0; 16_000 * 6 / 10]);
        c.samples.extend(speech(4.0).samples);
        c
    }

    fn piece_rig(replies: Vec<Result<String, ProviderError>>) -> Rig {
        let mut p = FakeProvider::new(Ok("x".into()), Ok("x".into()));
        p.direct_reply = Some("Whole.".into());
        *p.direct_replies.lock().unwrap() = replies.into();
        rig(p, true)
    }

    async fn yield_to_jobs() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn long_recording_is_processed_in_pieces_while_speaking() {
        let mut r = piece_rig(vec![Ok("Hello.".into()), Ok("World.".into())]);
        let t0 = Instant::now();
        *r.audio.live.lock().unwrap() = Some(long_speech());
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Translate)), t0).await;
        r.orch.handle(Input::Tick, t0 + Duration::from_secs(13)).await;
        yield_to_jobs().await;
        assert_eq!(r.provider.direct_calls.load(Ordering::SeqCst), 1, "first piece sent while still speaking");
        r.audio.captures.lock().unwrap().push_back(long_speech());
        r.orch.handle(Input::Hotkey(HotkeyEvent::Up(Mode::Translate)), t0 + Duration::from_secs(17)).await;
        r.settle(1).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Hello. World.".to_string()]);
        assert_eq!(r.provider.direct_calls.load(Ordering::SeqCst), 2);
        let u = r.shared.usage.lock().unwrap().today(chrono::Local::now().date_naive());
        assert_eq!(u.stt_requests, 2, "each piece is a request");
    }

    #[tokio::test]
    async fn failed_piece_falls_back_to_one_request() {
        // A piece that never got an answer (a 5xx gets a plain transcript inside the pipeline instead).
        // Groq has no backup routes, which would otherwise rescue the piece.
        let mut r = piece_rig(vec![Err(ProviderError::Timeout), Ok("Tail.".into())]);
        r.shared.settings.lock().unwrap().provider = ProviderKind::Groq;
        r.secrets.set(ProviderKind::Groq, "groq-key").unwrap();
        let t0 = Instant::now();
        *r.audio.live.lock().unwrap() = Some(long_speech());
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Translate)), t0).await;
        r.orch.handle(Input::Tick, t0 + Duration::from_secs(13)).await;
        yield_to_jobs().await;
        r.audio.captures.lock().unwrap().push_back(long_speech());
        r.orch.handle(Input::Hotkey(HotkeyEvent::Up(Mode::Translate)), t0 + Duration::from_secs(17)).await;
        r.settle(1).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Whole.".to_string()]);
    }

    #[tokio::test]
    async fn paragraphs_keep_one_request() {
        let mut r = piece_rig(vec![]);
        r.shared.settings.lock().unwrap().paragraphs = true;
        let t0 = Instant::now();
        *r.audio.live.lock().unwrap() = Some(long_speech());
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Translate)), t0).await;
        r.orch.handle(Input::Tick, t0 + Duration::from_secs(13)).await;
        yield_to_jobs().await;
        assert_eq!(r.provider.direct_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn cancelled_recording_drops_its_pieces() {
        let mut r = piece_rig(vec![Ok("Hello.".into())]);
        let t0 = Instant::now();
        *r.audio.live.lock().unwrap() = Some(long_speech());
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Translate)), t0).await;
        r.orch.handle(Input::Tick, t0 + Duration::from_secs(13)).await;
        r.orch.handle(Input::Hotkey(HotkeyEvent::Escape), t0 + Duration::from_secs(14)).await;
        yield_to_jobs().await;
        assert!(r.out.0.lock().unwrap().is_empty());
        assert_eq!(r.orch.in_flight, 0);
    }

    #[tokio::test]
    async fn long_tail_is_split_into_parallel_halves() {
        let mut r = piece_rig(vec![Ok("A.".into()), Ok("B.".into())]);
        let mut c = speech(5.0);
        c.samples.extend(vec![0.0; 16_000 * 4 / 10]);
        c.samples.extend(speech(5.0).samples);
        r.record(Mode::Translate, c, Instant::now(), 11).await;
        r.settle(1).await;
        assert_eq!(r.provider.direct_calls.load(Ordering::SeqCst), 2);
        let out = r.out.0.lock().unwrap().clone();
        let mut words: Vec<&str> = out[0].split(' ').collect();
        words.sort();
        assert_eq!(words, vec!["A.", "B."]);
    }

    #[tokio::test]
    async fn short_recording_stays_one_request() {
        let mut r = piece_rig(vec![]);
        r.record(Mode::Translate, speech(4.0), Instant::now(), 4).await;
        r.settle(1).await;
        assert_eq!(r.provider.direct_calls.load(Ordering::SeqCst), 1);
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Whole.".to_string()]);
    }

    /// A Live API stand-in: `script` arrives at once, `after_end` once the audio stream ends.
    #[derive(Default)]
    struct FakeLive {
        script: Vec<crate::providers::gemini_live::LiveEvent>,
        after_end: Vec<crate::providers::gemini_live::LiveEvent>,
        fail: Option<ProviderError>,
        connects: AtomicUsize,
        audio_samples: Arc<AtomicUsize>,
        /// `after_end` arrives this long after the stream ends.
        after_end_delay: Duration,
    }
    #[async_trait::async_trait]
    impl crate::providers::gemini_live::LiveConnector for FakeLive {
        async fn connect(&self, _: crate::providers::gemini_live::LiveConfig) -> Result<crate::providers::gemini_live::LiveSession, ProviderError> {
            self.connects.fetch_add(1, Ordering::SeqCst);
            if let Some(e) = &self.fail {
                return Err(e.clone());
            }
            let (audio_tx, mut audio_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<i16>>();
            let (ev_tx, ev_rx) = tokio::sync::mpsc::unbounded_channel();
            for e in &self.script {
                ev_tx.send(e.clone()).unwrap();
            }
            let after = self.after_end.clone();
            let samples = self.audio_samples.clone();
            let delay = self.after_end_delay;
            tokio::spawn(async move {
                while let Some(chunk) = audio_rx.recv().await {
                    if chunk.is_empty() {
                        tokio::time::sleep(delay).await;
                        for e in after {
                            let _ = ev_tx.send(e);
                        }
                        break;
                    }
                    samples.fetch_add(chunk.len(), Ordering::SeqCst);
                }
                // Keep the events channel open until the session is dropped.
                std::future::pending::<()>().await;
            });
            Ok(crate::providers::gemini_live::LiveSession { audio: audio_tx, events: ev_rx })
        }
    }

    use crate::providers::gemini_live::LiveEvent;

    fn live_rig(live: FakeLive) -> (Rig, Arc<FakeLive>) {
        let r = rig(FakeProvider::new(Ok("пакетный".into()), Ok("пакетный".into())), true);
        let live = Arc::new(live);
        let mut r = r;
        r.orch.deps.live = Some(live.clone());
        *r.audio.live.lock().unwrap() = Some(Captured { samples: Vec::new(), rate: 16_000, channels: 1 });
        (r, live)
    }

    fn heard(text: &str) -> FakeLive {
        FakeLive {
            script: vec![LiveEvent::Ready, LiveEvent::Interim(text.to_lowercase()), LiveEvent::Segment(text.into())],
            after_end: vec![LiveEvent::TurnComplete],
            ..Default::default()
        }
    }

    fn drafts(r: &Rig) -> Vec<Option<crate::live::draft::DraftParts>> {
        r.overlay.1.lock().unwrap().clone()
    }

    #[tokio::test]
    async fn gemini_dictation_goes_live() {
        let (mut r, live) = live_rig(heard("Привет, мир."));
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Привет, мир.".to_string()]);
        assert_eq!(live.connects.load(Ordering::SeqCst), 1);
        assert!(live.audio_samples.load(Ordering::SeqCst) > 0, "the recording was streamed");
        assert_eq!(r.provider.stt_calls.load(Ordering::SeqCst), 0, "no batch request");
        let h = r.shared.history.lock().unwrap();
        assert_eq!(h.entries().front().map(|e| (e.text.as_str(), e.raw.as_str())), Some(("Привет, мир.", "Привет, мир.")));
        drop(h);
        let d = drafts(&r);
        assert!(d.iter().any(Option::is_some), "the island showed the draft");
        assert_eq!(d.last(), Some(&None));
    }

    #[tokio::test]
    async fn slow_live_cleanup_is_raced_by_the_backup_lite_model() {
        // Live check 2026-11-07: the cleanup model answered in 0.6–0.8 s or in 2–3 s, at random.
        let (mut r, _) = live_rig(heard("привет э-э мир"));
        r.shared.settings.lock().unwrap().cleanup = true;
        let f = RouteFactory::texts(vec![(Duration::from_millis(2_500), "Медленно."), (Duration::from_millis(50), "Быстро."), (Duration::ZERO, "c")]);
        r.orch.set_provider_factory(f.clone());
        let started = std::time::Instant::now();
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Быстро.".to_string()]);
        assert!(started.elapsed() < Duration::from_millis(2_300), "{:?}", started.elapsed());
        assert!(f.made.lock().unwrap().contains(&Route::Direct { model: defaults::GEMINI_BACKUP_LITE_MODEL }));
    }

    #[tokio::test]
    async fn late_draft_of_previous_dictation_is_dropped() {
        // Released A is still waiting for its final piece when B starts (batch: A is in flight).
        let (mut r, _) = live_rig(FakeLive {
            script: vec![LiveEvent::Interim("первая".into())],
            after_end: vec![LiveEvent::Interim("первая поздно".into()), LiveEvent::Segment("Первая поздно.".into())],
            after_end_delay: Duration::from_millis(300),
            ..Default::default()
        });
        let t0 = Instant::now();
        r.record(Mode::Dictation, speech(1.0), t0, 1).await;
        r.audio.captures.lock().unwrap().push_back(speech(1.0));
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Dictation)), t0 + Duration::from_secs(2)).await;
        let from = drafts(&r).len();
        tokio::time::sleep(Duration::from_millis(600)).await;
        // B goes live too (with its own "первая" from the same fake); A's late words must not show.
        let late: Vec<_> = drafts(&r)[from..].iter().flatten().filter(|d| d.settled.contains("поздно") || d.interim.contains("поздно")).cloned().collect();
        assert!(late.is_empty(), "A's draft reached B's island: {late:?}");
    }

    #[tokio::test]
    async fn failed_live_resumes_pieces() {
        // The live connection fails at the start of a long dictation: the pieces path takes over
        // while the user is still speaking, instead of one long request after release.
        let mut r = piece_rig(vec![Ok("Hello.".into()), Ok("World.".into())]);
        let live = Arc::new(FakeLive { fail: Some(ProviderError::Network), ..Default::default() });
        r.orch.deps.live = Some(live.clone());
        let t0 = Instant::now();
        *r.audio.live.lock().unwrap() = Some(long_speech());
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Translate)), t0).await;
        yield_to_jobs().await;
        assert_eq!(live.connects.load(Ordering::SeqCst), 1);
        r.orch.handle(Input::Tick, t0 + Duration::from_secs(13)).await;
        yield_to_jobs().await;
        assert_eq!(r.provider.direct_calls.load(Ordering::SeqCst), 1, "first piece sent while still speaking");
        r.audio.captures.lock().unwrap().push_back(long_speech());
        r.orch.handle(Input::Hotkey(HotkeyEvent::Up(Mode::Translate)), t0 + Duration::from_secs(17)).await;
        r.settle(1).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Hello. World.".to_string()]);
        let d = drafts(&r);
        assert_eq!(d.iter().filter(|x| x.is_none()).count(), 1, "cleared once, after pasting: {d:?}");
    }

    #[tokio::test]
    async fn second_dictation_goes_live_while_first_is_processed() {
        // The user's decision 2026-11-07: the window shows for a dictation started while the
        // previous one is still processed; the ordered queue keeps the insertion order.
        let (mut r, live) = live_rig(heard("Текст."));
        let t0 = Instant::now();
        r.record(Mode::Dictation, speech(1.0), t0, 1).await;
        r.record(Mode::Dictation, speech(1.0), t0 + Duration::from_secs(2), 1).await;
        r.settle(2).await;
        assert_eq!(live.connects.load(Ordering::SeqCst), 2, "both dictations went live");
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Текст.".to_string(), "Текст.".to_string()]);
    }

    #[tokio::test]
    async fn finishing_previous_job_keeps_current_window() {
        // A is released and still waiting for its final piece when B starts with its own window.
        let (mut r, _) = live_rig(FakeLive {
            script: vec![LiveEvent::Interim("слова".into())],
            after_end: vec![LiveEvent::Segment("Слова.".into())],
            after_end_delay: Duration::from_millis(200),
            ..Default::default()
        });
        let t0 = Instant::now();
        r.record(Mode::Dictation, speech(1.0), t0, 1).await;
        r.audio.captures.lock().unwrap().push_back(speech(1.0));
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Dictation)), t0 + Duration::from_secs(2)).await;
        let from = drafts(&r).len();
        r.settle(1).await; // A is pasted while B is still recording
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Слова.".to_string()]);
        let during_b = drafts(&r)[from..].to_vec();
        assert!(during_b.iter().any(Option::is_some), "B shows its own draft");
        assert!(!during_b.contains(&None), "finishing A must not close B's window: {during_b:?}");
        r.orch.handle(Input::Hotkey(HotkeyEvent::Up(Mode::Dictation)), t0 + Duration::from_secs(4)).await;
        r.settle(1).await;
        assert_eq!(drafts(&r).last(), Some(&None), "B's window closes when B is pasted");
    }

    #[tokio::test]
    async fn live_fallback_runs_batch_with_recorded_audio() {
        let (mut r, live) = live_rig(FakeLive { fail: Some(ProviderError::Network), ..Default::default() });
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(live.connects.load(Ordering::SeqCst), 1);
        assert_eq!(*r.out.0.lock().unwrap(), vec!["пакетный".to_string()]);
        assert_eq!(r.provider.stt_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn microphone_change_mid_live_dictation_takes_the_whole_recording() {
        // Review 2026-11-07: after a device change the recorder starts a new segment and the
        // live stream gets nothing more; the cut-off live text was pasted as final.
        let (mut r, _) = live_rig(heard("Обрывок."));
        let t0 = Instant::now();
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Dictation)), t0).await;
        r.orch.handle(Input::Tick, t0 + Duration::from_millis(200)).await;
        *r.audio.live.lock().unwrap() = None; // the headphones were unplugged
        r.orch.handle(Input::Tick, t0 + Duration::from_millis(400)).await;
        r.audio.captures.lock().unwrap().push_back(speech(1.0));
        r.orch.handle(Input::Hotkey(HotkeyEvent::Up(Mode::Dictation)), t0 + Duration::from_secs(1)).await;
        r.settle(1).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["пакетный".to_string()]);
        assert_eq!(r.provider.stt_calls.load(Ordering::SeqCst), 1, "the whole recording went in one request");
        assert_eq!(drafts(&r).last(), Some(&None), "the window closes with the paste");
    }

    #[tokio::test]
    async fn mid_speech_drop_falls_back_keeping_window() {
        let (mut r, live) = live_rig(FakeLive {
            script: vec![LiveEvent::Interim("начало".into()), LiveEvent::Error("closed".into())],
            ..Default::default()
        });
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(live.connects.load(Ordering::SeqCst), 1);
        assert_eq!(*r.out.0.lock().unwrap(), vec!["пакетный".to_string()]);
        let d = drafts(&r);
        let first_none = d.iter().position(Option::is_none).expect("cleared at the end");
        assert_eq!(first_none, d.len() - 1, "the window stayed until the batch text was pasted: {d:?}");
    }

    #[tokio::test]
    async fn translate_ignores_cleanup_off() {
        let (mut r, live) = live_rig(heard("привет мир"));
        assert!(!r.shared.settings().cleanup);
        r.record(Mode::Translate, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(live.connects.load(Ordering::SeqCst), 1);
        assert_eq!(r.provider.stt_calls.load(Ordering::SeqCst), 0, "no batch request");
        // The fake text model answers "Привет." to anything: the text went through it.
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Привет.".to_string()]);
    }

    #[tokio::test]
    async fn unfinished_cleanup_uses_routes() {
        let (mut r, _) = live_rig(FakeLive::default());
        r.shared.settings.lock().unwrap().cleanup = true;
        let t0 = Instant::now();
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Dictation)), t0).await;
        r.audio.captures.lock().unwrap().push_back(speech(1.0));
        r.orch.handle(Input::Hotkey(HotkeyEvent::Up(Mode::Dictation)), t0 + Duration::from_secs(1)).await;
        r.orch.handle(Input::LiveDone(1, Box::new(LiveOutcome::Unfinished { raw: "сырой текст".into(), requests: 1 })), Instant::now()).await;
        r.settle(1).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["Привет.".to_string()]);
    }

    #[tokio::test]
    async fn unfinished_cleanup_fails_gives_raw_degraded() {
        let (mut r, _) = live_rig(FakeLive::default());
        r.shared.settings.lock().unwrap().cleanup = true;
        let mut p = FakeProvider::new(Ok("x".into()), Ok("x".into()));
        p.text_fail = true;
        r.orch.set_provider_factory(Arc::new(FakeFactory(Arc::new(p))));
        let t0 = Instant::now();
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Dictation)), t0).await;
        r.audio.captures.lock().unwrap().push_back(speech(1.0));
        r.orch.handle(Input::Hotkey(HotkeyEvent::Up(Mode::Dictation)), t0 + Duration::from_secs(1)).await;
        r.orch.handle(Input::LiveDone(1, Box::new(LiveOutcome::Unfinished { raw: "сырой текст".into(), requests: 1 })), Instant::now()).await;
        r.settle(1).await;
        assert_eq!(*r.out.0.lock().unwrap(), vec!["сырой текст".to_string()]);
        let h = r.shared.history.lock().unwrap();
        assert_eq!(h.entries().front().map(|e| e.status.clone()), Some(EntryStatus::Degraded));
    }

    async fn unfinished(r: &mut Rig, mode: Mode) {
        let t0 = Instant::now();
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(mode)), t0).await;
        r.audio.captures.lock().unwrap().push_back(speech(1.0));
        r.orch.handle(Input::Hotkey(HotkeyEvent::Up(mode)), t0 + Duration::from_secs(1)).await;
        r.orch.handle(Input::LiveDone(1, Box::new(LiveOutcome::Unfinished { raw: "сырой текст".into(), requests: 1 })), Instant::now()).await;
    }

    #[tokio::test]
    async fn crash_in_finish_text_does_not_stall_queue() {
        let (mut r, _) = live_rig(FakeLive::default());
        r.shared.settings.lock().unwrap().cleanup = true;
        let mut p = FakeProvider::new(Ok("x".into()), Ok("x".into()));
        p.text_panic = true;
        r.orch.set_provider_factory(Arc::new(FakeFactory(Arc::new(p))));
        unfinished(&mut r, Mode::Dictation).await;
        r.settle(1).await;
        assert!(r.out.0.lock().unwrap().is_empty());
        assert!(r.overlay.last_error().is_some(), "an honest error instead of an endless 'processing'");
        assert_eq!(r.orch.in_flight, 0);
    }

    #[tokio::test]
    async fn failed_live_translate_keeps_audio_for_retry() {
        let (mut r, _) = live_rig(FakeLive::default());
        let mut p = FakeProvider::new(Err(ProviderError::Server(500)), Err(ProviderError::Server(500)));
        p.text_fail = true;
        r.orch.set_provider_factory(Arc::new(FakeFactory(Arc::new(p))));
        unfinished(&mut r, Mode::Translate).await;
        r.settle(1).await;
        assert!(r.out.0.lock().unwrap().is_empty());
        let h = r.shared.history.lock().unwrap();
        let audio = h.entries().front().and_then(|e| e.audio_file.clone());
        assert!(audio.is_some_and(|p| p.exists()), "the recording stays for Retry");
    }

    #[tokio::test]
    async fn empty_live_is_nothing_heard() {
        let (mut r, live) = live_rig(FakeLive { after_end: vec![LiveEvent::TurnComplete], ..Default::default() });
        let silence = Captured { samples: vec![0.0; 16_000], rate: 16_000, channels: 1 };
        r.record(Mode::Dictation, silence, Instant::now(), 1).await;
        r.settle(1).await;
        assert!(r.out.0.lock().unwrap().is_empty());
        assert_eq!(r.overlay.last_error().as_deref(), Some("nothing_heard"));
        assert_eq!(live.connects.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn esc_cancels_live() {
        let (mut r, live) = live_rig(heard("Не нужно."));
        r.orch.handle(Input::Hotkey(HotkeyEvent::Down(Mode::Dictation)), Instant::now()).await;
        tokio::time::sleep(Duration::from_millis(30)).await;
        r.orch.handle(Input::Hotkey(HotkeyEvent::Escape), Instant::now()).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(live.connects.load(Ordering::SeqCst), 1);
        assert!(r.out.0.lock().unwrap().is_empty());
        assert_eq!(drafts(&r).last(), Some(&None));
        assert_eq!(r.overlay.0.lock().unwrap().last(), Some(&OverlayState::Cancelled));
    }

    #[tokio::test]
    async fn groq_stays_batch() {
        let (mut r, live) = live_rig(heard("x"));
        r.shared.settings.lock().unwrap().provider = ProviderKind::Groq;
        r.secrets.set(ProviderKind::Groq, "groq-key").unwrap();
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(live.connects.load(Ordering::SeqCst), 0);
        assert_eq!(*r.out.0.lock().unwrap(), vec!["пакетный".to_string()]);
    }

    #[tokio::test]
    async fn draft_cleared_on_every_end() {
        let (mut r, live) = live_rig(FakeLive { fail: Some(ProviderError::Server(503)), ..Default::default() });
        r.record(Mode::Dictation, speech(1.0), Instant::now(), 1).await;
        r.settle(1).await;
        assert_eq!(live.connects.load(Ordering::SeqCst), 1);
        assert_eq!(drafts(&r).last(), Some(&None));
    }

    #[test]
    fn timeout_is_slow_provider() {
        assert!(matches!(error_state(&ProviderError::Timeout, ProviderKind::Gemini), OverlayState::Error { message_key, .. } if message_key == "slow_provider"));
        assert!(matches!(error_state(&ProviderError::Network, ProviderKind::Gemini), OverlayState::Error { message_key, .. } if message_key == "network"));
    }

    #[test]
    fn location_error_is_region_blocked() {
        let e = ProviderError::BadResponse("User location is not supported for the API use.".into());
        assert!(matches!(error_state(&e, ProviderKind::Gemini), OverlayState::Error { message_key, .. } if message_key == "region_blocked"));
    }
}
