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
use crate::output::{CopyReason, DeliveryOutcome};
use crate::permissions::PermStatus;
use crate::pipeline::{self, JobError, JobOutput, PipelineOptions};
use crate::providers::{AudioLlm, ProviderError, SpeechToText, TextModel};
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

type Piece = tokio::task::JoinHandle<Result<JobOutput, JobError>>;

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
    provider: Option<(ProviderKind, String, Arc<dyn Provider>)>,
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
        ProviderError::Network | ProviderError::Timeout => OverlayState::error("network", None),
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
            wall_clock: Box::new(SystemTime::now),
            last_tick_wall: None,
            last_hotkey_check: None,
            status: AppStatus::Idle,
            pieces: Vec::new(),
            piece_offset: 0,
            piece_layout: None,
            last_piece_check: None,
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
                for (_, r) in self.queue.push(id, *result) {
                    self.finish(r).await;
                }
            }
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
        self.check_pieces(now);
    }

    /// Sends the finished part of a long recording at a pause, while the user keeps speaking.
    fn check_pieces(&mut self, now: Instant) {
        let Some((mode, _)) = self.recording else { return };
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

    fn provider_for(&mut self, s: &Settings) -> Option<Arc<dyn Provider>> {
        let key = self.deps.secrets.get(s.provider)?;
        if let Some((kind, k, p)) = &self.provider {
            if *kind == s.provider && *k == key {
                return Some(p.clone());
            }
        }
        let p = self.deps.providers.make(s.provider, key.clone());
        self.provider = Some((s.provider, key, p.clone()));
        Some(p)
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
        tokio::spawn(async move { provider.prewarm().await });
    }

    fn submit(&mut self, mode: Mode) {
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
        self.spawn_job(mode, AudioInput::Captured(captured), None, split);
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

    fn spawn_job(&mut self, mode: Mode, audio_in: AudioInput, history_id: Option<u64>, split: Option<Split>) {
        let s = self.shared.settings();
        let id = self.ids.next();
        if self.in_flight == 0 {
            self.processing_since = now_ms();
        }
        self.in_flight += 1;
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
            let mut requests = 1;
            let mut outcome = None;
            let split = split.map(|sp| (sp.pieces, split_tail(sp.rest)));
            // A short recording sent nowhere yet gains nothing from the piecewise path.
            let split = split.filter(|(pieces, tail)| !pieces.is_empty() || tail.len() > 1);
            if let Some((pieces, tail)) = split {
                let mut outs = Vec::new();
                for p in pieces {
                    outs.push(p.await.unwrap_or(Err(JobError::Provider(ProviderError::BadResponse("piece crashed".into())))));
                }
                // The halves of a long tail run at the same time.
                let tail: Vec<_> = tail
                    .into_iter()
                    .map(|c| tokio::spawn(process_capture(mode, c, opts.clone(), provider.clone())))
                    .collect();
                for t in tail {
                    outs.push(t.await.unwrap_or(Err(JobError::Provider(ProviderError::BadResponse("piece crashed".into())))));
                }
                requests = outs.len() as u32;
                if outs.iter().any(|o| matches!(o, Err(JobError::Provider(_)))) {
                    tracing::warn!("a piece failed; sending the whole recording in one request");
                    requests += 1;
                } else {
                    outcome = Some(combine(outs));
                }
            }
            let outcome = match outcome {
                Some(o) => o,
                None => match wav {
                    None => Err(JobError::NothingHeard),
                    Some(wav) => process(mode, wav, secs, &opts, provider).await,
                },
            };
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

    async fn finish(&mut self, r: JobResult) {
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

fn pipeline_options(s: &Settings) -> PipelineOptions {
    PipelineOptions {
        language: s.speech_language.clone(),
        cleanup: s.cleanup,
        paragraphs: s.paragraphs,
        style: s.translate_style,
    }
}

/// One speech request (plus the LLM step when the provider needs a second one).
async fn process(
    mode: Mode,
    wav: Vec<u8>,
    secs: f32,
    opts: &PipelineOptions,
    provider: Option<Arc<dyn Provider>>,
) -> Result<JobOutput, JobError> {
    let Some(p) = provider else { return Err(JobError::Provider(ProviderError::Unauthorized)) };
    match p.direct() {
        Some(direct) if pipeline::audio_system_prompt(mode, opts, secs).is_some() => {
            pipeline::run_direct(mode, wav, secs, opts, direct, p.as_ref()).await
        }
        _ => pipeline::run(mode, wav, secs, opts, p.as_ref(), p.as_ref()).await,
    }
}

/// Prepares raw audio off the async threads, then processes it.
async fn process_capture(
    mode: Mode,
    c: Captured,
    opts: PipelineOptions,
    provider: Option<Arc<dyn Provider>>,
) -> Result<JobOutput, JobError> {
    let prepared = tokio::task::spawn_blocking(move || audio::prepare(c)).await.ok().flatten();
    match prepared {
        Some(p) => process(mode, p.wav, p.audio_secs, &opts, provider).await,
        None => Err(JobError::NothingHeard),
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
            }
        }
    }
    #[async_trait::async_trait]
    impl AudioLlm for FakeProvider {
        async fn complete_audio(&self, _: &str, _: Vec<u8>, _: Option<String>, _: f32) -> Result<Completion, ProviderError> {
            self.direct_calls.fetch_add(1, Ordering::SeqCst);
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
            Ok(Completion { text: "Привет.".into(), total_tokens: 3 })
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

    #[derive(Default)]
    struct FakeOutput(Mutex<Vec<String>>);
    impl Deliverer for FakeOutput {
        fn deliver(&self, text: &str, _: InsertMethod) -> DeliveryOutcome {
            self.0.lock().unwrap().push(text.to_string());
            DeliveryOutcome::Pasted
        }
    }

    #[derive(Default)]
    struct FakeOverlay(Mutex<Vec<OverlayState>>);
    impl OverlaySink for FakeOverlay {
        fn show(&self, s: &OverlayState) {
            self.0.lock().unwrap().push(s.clone());
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
            secrets,
            audio: audio.clone(),
            providers: Arc::new(FakeFactory(provider.clone())),
            output: out.clone(),
            overlay: overlay.clone(),
            permissions: perms.clone(),
            hotkeys: hotkeys.clone(),
            ui: ui.clone(),
        };
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let orch = Orchestrator::new(deps, shared.clone(), tx);
        Rig { orch, rx, audio, provider, out, overlay, shared, hotkeys, perms, ui, _dir: dir }
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
        let mut r = piece_rig(vec![Err(ProviderError::Timeout), Ok("Tail.".into())]);
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
}
