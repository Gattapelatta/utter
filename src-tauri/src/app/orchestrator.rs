//! Wires hotkeys → recorder → pipeline → output. One task owns all mutable state;
//! everything that talks to the OS sits behind a trait so the flow is testable.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::mpsc::UnboundedSender;

use crate::audio::{self, Captured};
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

const DONE_VISIBLE: Duration = Duration::from_millis(1200);
const ERROR_VISIBLE: Duration = Duration::from_millis(2500);
/// A gap this long between 100 ms ticks means the machine was asleep.
const WAKE_GAP: Duration = Duration::from_secs(10);
const HOTKEY_CHECK: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OverlayState {
    Hidden,
    Recording { mode: Mode, style: TranslateStyle, started_at_ms: i64, warn: bool, pending: u32 },
    Processing { pending: u32 },
    Done { outcome: String, degraded: bool },
    Error { message_key: String, action: Option<String>, minutes: Option<u32> },
}

impl OverlayState {
    pub fn kind(&self) -> &'static str {
        match self {
            OverlayState::Hidden => "hidden",
            OverlayState::Recording { .. } => "recording",
            OverlayState::Processing { .. } => "processing",
            OverlayState::Done { .. } => "done",
            OverlayState::Error { .. } => "error",
        }
    }

    fn error(key: &str, action: Option<&str>) -> Self {
        OverlayState::Error { message_key: key.into(), action: action.map(Into::into), minutes: None }
    }
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
}

/// State the settings UI reads and writes directly.
pub struct Shared {
    pub settings: Mutex<Settings>,
    pub history: Mutex<History>,
    pub usage: Mutex<Usage>,
    pub data_dir: PathBuf,
}

impl Shared {
    pub fn new(settings: Settings, data_dir: PathBuf) -> Self {
        Self {
            history: Mutex::new(History::load(&data_dir.join("history.json"))),
            usage: Mutex::new(Usage::load(&data_dir.join("usage.json"))),
            settings: Mutex::new(settings),
            data_dir,
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
    outcome: Result<JobOutput, JobError>,
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
    provider: Option<(ProviderKind, String, Arc<dyn Provider>)>,
    last_tick: Option<Instant>,
    last_hotkey_check: Option<Instant>,
}

fn now_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

fn error_state(e: &ProviderError) -> OverlayState {
    match e {
        ProviderError::Unauthorized => OverlayState::error("bad_key", Some("open_provider")),
        ProviderError::RateLimited { retry_after } => OverlayState::Error {
            message_key: "rate_limited".into(),
            action: None,
            minutes: retry_after.map(|d| d.as_secs().div_ceil(60).max(1) as u32),
        },
        ProviderError::Network | ProviderError::Timeout => OverlayState::error("network", None),
        ProviderError::Server(_) | ProviderError::BadResponse(_) => OverlayState::error("service_down", None),
    }
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
            provider: None,
            last_tick: None,
            last_hotkey_check: None,
        }
    }

    pub fn controller_state(&self) -> &State {
        self.controller.state()
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
        match input {
            Input::Hotkey(ev) => {
                let cmds = self.controller.handle(ev, now);
                self.exec(cmds).await;
            }
            Input::Tick => self.tick(now).await,
            Input::JobDone(id, result) => {
                self.in_flight = self.in_flight.saturating_sub(1);
                for (_, r) in self.queue.push(id, *result) {
                    self.finish(r).await;
                }
            }
            Input::HideOverlay(seq) => {
                if seq == self.overlay_seq && self.recording.is_none() {
                    self.deps.overlay.show(&OverlayState::Hidden);
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
        if let Some(last) = self.last_tick {
            if now.saturating_duration_since(last) > WAKE_GAP {
                tracing::info!("woke from sleep; restarting hotkeys and HTTP client");
                self.deps.hotkeys.restart();
                self.provider = None;
            }
        }
        self.last_tick = Some(now);
        let due = self.last_hotkey_check.is_none_or(|t| now.saturating_duration_since(t) >= HOTKEY_CHECK);
        if due {
            self.last_hotkey_check = Some(now);
            self.deps.hotkeys.ensure_running();
        }
        let cmds = self.controller.tick(now);
        self.exec(cmds).await;
    }

    fn show(&mut self, s: OverlayState) {
        self.overlay_seq += 1;
        self.deps.overlay.show(&s);
    }

    /// Shows a transient result unless a recording is on screen, then hides it later.
    fn show_result(&mut self, s: OverlayState, visible: Duration) {
        if self.recording.is_some() {
            return;
        }
        if self.in_flight > 0 && matches!(s, OverlayState::Done { .. }) {
            self.show(OverlayState::Processing { pending: self.in_flight });
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
                Command::Cancel(_) => {
                    self.deps.audio.cancel();
                    self.recording = None;
                    if self.in_flight > 0 {
                        self.show(OverlayState::Processing { pending: self.in_flight });
                    } else {
                        self.show(OverlayState::Hidden);
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

    fn fail_start(&mut self, state: OverlayState) {
        self.controller.abort();
        self.recording = None;
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
        if let Err(e) = self.deps.audio.start() {
            tracing::warn!("recorder failed to start: {e}");
            return self.fail_start(OverlayState::error("no_microphone", None));
        }
        let started = now_ms();
        self.recording = Some((mode, started));
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
        self.recording = None;
        let captured = self.deps.audio.stop().unwrap_or_default();
        self.spawn_job(mode, AudioInput::Captured(captured), None);
    }

    fn retry(&mut self, history_id: u64) {
        let entry = self.shared.history.lock().ok().and_then(|h| h.get(history_id).cloned());
        let Some(entry) = entry else { return };
        let Some(path) = entry.audio_file.clone() else { return };
        let Ok(bytes) = std::fs::read(&path) else { return };
        // 16 kHz mono 16-bit: 32 000 bytes per second after the 44-byte header.
        let secs = bytes.len().saturating_sub(44) as f32 / 32_000.0;
        self.spawn_job(entry.mode, AudioInput::Wav { bytes, secs }, Some(history_id));
    }

    fn spawn_job(&mut self, mode: Mode, audio_in: AudioInput, history_id: Option<u64>) {
        let s = self.shared.settings();
        let id = self.ids.next();
        self.in_flight += 1;
        if self.recording.is_none() {
            self.show(OverlayState::Processing { pending: self.in_flight });
        }
        let provider = self.provider_for(&s);
        let audio_dir = self.shared.audio_dir();
        let tx = self.tx.clone();
        let opts = PipelineOptions {
            language: s.speech_language.clone(),
            cleanup: s.cleanup,
            paragraphs: s.paragraphs,
            style: s.translate_style,
        };
        let created_at_ms = now_ms();
        let submitted = Instant::now();
        tokio::spawn(async move {
            let (wav, secs, audio_path) = match audio_in {
                AudioInput::Wav { bytes, secs } => (Some(bytes), secs, None),
                AudioInput::Captured(c) => {
                    let t_prep = Instant::now();
                    let prepared = tokio::task::spawn_blocking(move || audio::prepare(c)).await.ok().flatten();
                    tracing::info!(prepare_ms = t_prep.elapsed().as_millis() as u64, "timing: audio prepare");
                    match prepared {
                        Some(p) => {
                            let path = audio_dir.join(format!("{id}.wav"));
                            let saved = std::fs::create_dir_all(&audio_dir)
                                .and_then(|_| std::fs::write(&path, &p.wav))
                                .is_ok();
                            (Some(p.wav), p.audio_secs, saved.then_some(path))
                        }
                        None => (None, 0.0, None),
                    }
                }
            };
            let outcome = match (wav, provider) {
                (None, _) => Err(JobError::NothingHeard),
                (Some(_), None) => Err(JobError::Provider(ProviderError::Unauthorized)),
                (Some(wav), Some(p)) => match p.direct() {
                    Some(direct) if pipeline::audio_system_prompt(mode, &opts, secs).is_some() => {
                        pipeline::run_direct(mode, wav, secs, &opts, direct, p.as_ref()).await
                    }
                    _ => pipeline::run(mode, wav, secs, &opts, p.as_ref(), p.as_ref()).await,
                },
            };
            let result = JobResult { mode, submitted, history_id, audio_path, audio_secs: secs, created_at_ms, outcome };
            let _ = tx.send(Input::JobDone(id, Box::new(result)));
        });
    }

    async fn finish(&mut self, r: JobResult) {
        let s = self.shared.settings();
        let today = chrono::Local::now().date_naive();
        let stt_called = r.audio_secs > 0.0;
        if let Ok(mut u) = self.shared.usage.lock() {
            if stt_called && !matches!(r.outcome, Err(JobError::Provider(_))) {
                u.record_stt(today, r.audio_secs);
            }
            if let Ok(out) = &r.outcome {
                if out.llm_used && !out.degraded {
                    u.record_llm(today, out.tokens);
                }
            }
            let _ = u.save();
        }

        let (status, text, raw, keep_audio, overlay, visible) = match r.outcome {
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
                EntryStatus::Failed { message: e.to_string() },
                String::new(),
                String::new(),
                true,
                error_state(&e),
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
    }

    /// Long audio (> 1.5 s) is answered slowly with `slow`, short audio quickly with `fast`.
    struct FakeProvider {
        slow: Result<String, ProviderError>,
        fast: Result<String, ProviderError>,
        stt_calls: AtomicUsize,
        /// When set, the provider offers a one-request audio→text path returning this.
        direct_reply: Option<String>,
        direct_calls: AtomicUsize,
    }
    impl FakeProvider {
        fn new(slow: Result<String, ProviderError>, fast: Result<String, ProviderError>) -> Self {
            Self { slow, fast, stt_calls: AtomicUsize::new(0), direct_reply: None, direct_calls: AtomicUsize::new(0) }
        }
    }
    #[async_trait::async_trait]
    impl AudioLlm for FakeProvider {
        async fn complete_audio(&self, _: &str, _: Vec<u8>, _: Option<String>, _: f32) -> Result<Completion, ProviderError> {
            self.direct_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Completion { text: self.direct_reply.clone().unwrap_or_default(), total_tokens: 5 })
        }
    }
    #[async_trait::async_trait]
    impl SpeechToText for FakeProvider {
        async fn transcribe(&self, _: Vec<u8>, _: Option<String>, secs: f32) -> Result<String, ProviderError> {
            self.stt_calls.fetch_add(1, Ordering::SeqCst);
            if secs > 1.5 {
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

    struct AllGranted;
    impl PermissionCheck for AllGranted {
        fn accessibility(&self) -> bool {
            true
        }
        fn microphone(&self) -> PermStatus {
            PermStatus::Granted
        }
    }

    struct NoHotkeys;
    impl HotkeyControl for NoHotkeys {
        fn set_bindings(&self, _: crate::settings::Hotkeys) {}
        fn ensure_running(&self) -> bool {
            true
        }
        fn restart(&self) {}
    }

    struct Rig {
        orch: Orchestrator,
        rx: tokio::sync::mpsc::UnboundedReceiver<Input>,
        audio: Arc<FakeAudio>,
        provider: Arc<FakeProvider>,
        out: Arc<FakeOutput>,
        overlay: Arc<FakeOverlay>,
        shared: Arc<Shared>,
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
        let deps = Deps {
            secrets,
            audio: audio.clone(),
            providers: Arc::new(FakeFactory(provider.clone())),
            output: out.clone(),
            overlay: overlay.clone(),
            permissions: Arc::new(AllGranted),
            hotkeys: Arc::new(NoHotkeys),
        };
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let orch = Orchestrator::new(deps, shared.clone(), tx);
        Rig { orch, rx, audio, provider, out, overlay, shared, _dir: dir }
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
}
