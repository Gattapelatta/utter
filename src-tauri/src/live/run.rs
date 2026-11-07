//! One live dictation (spec 2026-11-06 §4): stream the recording to the Live API, show the draft
//! in the island, and on release produce the final text — cleaned or translated piece by piece —
//! or tell the orchestrator to take the batch path.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::Instant;

use super::draft::{DraftComposer, DraftParts, DraftThrottle};
use super::pcm::Chunker;
use super::translate::SegmentTranslator;
use crate::providers::gemini_live::{LiveConfig, LiveConnector, LiveEvent, LiveSession};
use crate::providers::{ProviderError, TextModel};

/// After release, the final pieces may take this long (spec 2026-11-04 §4.3).
pub const FINAL_WAIT: Duration = Duration::from_millis(1_500);
/// Google sends the last final piece ~0.4 s after the stream ends but no `turnComplete`
/// (measured 2026-11-06): once a final piece leaves nothing unheard, finish this soon after.
pub const SETTLE_AFTER_FINAL: Duration = Duration::from_millis(150);
/// Released after a pause: everything heard is final already and Google sends nothing more.
/// Late words would show as a draft within about one update (~0.5 s).
pub const QUIET_AT_RELEASE: Duration = Duration::from_millis(500);
/// The quiet wait is only taken if the voice stopped at least this long before the release:
/// words said right before letting go may have no draft yet, and their piece comes ~0.55 s
/// after the stream ends (review 2026-11-07).
pub const QUIET_TAIL: Duration = Duration::from_millis(700);
/// Less voice than this in `QUIET_TAIL` is a click or a breath, not words.
pub const TAIL_VOICE_MS: u32 = 120;
/// No word back from the server this long while the user spoke at least `STALL_VOICE_MS`: the
/// connection died without an error (review 2026-11-07); the recording takes the batch path.
pub const STALL_AFTER: Duration = Duration::from_secs(3);
pub const STALL_VOICE_MS: u32 = 1_500;

/// What turns the verbatim pieces into the text that is pasted.
pub enum Finisher {
    /// Dictation without cleanup: the pieces as heard.
    Plain,
    /// Cleanup or translation of the pieces while speaking (`SegmentTranslator`); `backup`
    /// races a slow or failed request at the end.
    Text { model: Arc<dyn TextModel>, system: String, backup: Option<Arc<dyn TextModel>> },
}

pub struct LiveRequest {
    pub cfg: LiveConfig,
    pub finisher: Finisher,
    pub rate: u32,
    pub channels: u16,
}

#[derive(Debug, PartialEq)]
pub enum LiveOutcome {
    Final { text: String, raw: String, requests: u32 },
    /// The verbatim text is there, its cleanup or translation did not come.
    Unfinished { raw: String, requests: u32 },
    /// Take the batch path with the recorded audio.
    Fallback { reason: String, error: Option<ProviderError> },
    Cancelled,
}

enum Cmd {
    Audio(Vec<f32>),
    Release,
    Cancel,
}

/// Controls a running live dictation; dropping it cancels.
pub struct LiveHandle {
    tx: mpsc::UnboundedSender<Cmd>,
    failed: Arc<AtomicBool>,
}

impl LiveHandle {
    /// The connection failed or dropped: this dictation will end in `Fallback`, so the
    /// orchestrator can take the batch path (pieces) while the user is still speaking.
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }
    /// Interleaved samples recorded since the last call.
    pub fn audio(&self, interleaved: &[f32]) {
        if !interleaved.is_empty() {
            let _ = self.tx.send(Cmd::Audio(interleaved.to_vec()));
        }
    }
    pub fn release(&self) {
        let _ = self.tx.send(Cmd::Release);
    }
    pub fn cancel(&self) {
        let _ = self.tx.send(Cmd::Cancel);
    }
}

/// The provider error a Live API failure stands for (the pill names it honestly).
pub fn error_of(reason: &str) -> ProviderError {
    if reason == "timeout" {
        ProviderError::Timeout
    } else {
        ProviderError::BadResponse(reason.to_string())
    }
}

/// Starts one live dictation. `draft` gets the island's draft (`None` only on cancel: on any other
/// outcome the window stays until the orchestrator has pasted the text); `done` gets the outcome once.
pub fn spawn(
    req: LiveRequest,
    connector: Arc<dyn LiveConnector>,
    draft: Arc<dyn Fn(Option<DraftParts>) + Send + Sync>,
    done: impl FnOnce(LiveOutcome) + Send + 'static,
) -> LiveHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let failed = Arc::new(AtomicBool::new(false));
    let flag = failed.clone();
    tokio::spawn(async move {
        // A bug in one dictation must end in the batch path, never in a stalled queue.
        let (d, f) = (draft.clone(), flag.clone());
        let work = tokio::spawn(async move { run(req, connector, d.as_ref(), rx, &f).await });
        let outcome = match work.await {
            Ok(o) => o,
            Err(e) => {
                tracing::error!("live dictation crashed: {e}");
                flag.store(true, Ordering::SeqCst);
                LiveOutcome::Fallback { reason: "crashed".into(), error: None }
            }
        };
        if outcome == LiveOutcome::Cancelled {
            draft(None);
        }
        if let LiveOutcome::Fallback { reason, .. } = &outcome {
            tracing::info!(reason = %reason, "live fallback");
        }
        done(outcome);
    });
    LiveHandle { tx, failed }
}

/// Notes how much voice the microphone heard and when (interleaved samples at `rate` × channels);
/// keeps only what the quiet check at release looks at.
fn heard(a: &[f32], rate: u32, log: &mut std::collections::VecDeque<(Instant, u32)>) {
    let ms = crate::audio::dsp::voice_stats(a, rate).voiced_ms;
    if ms > 0 {
        log.push_back((Instant::now(), ms));
    }
    while log.front().is_some_and(|(t, _)| t.elapsed() >= QUIET_TAIL) {
        log.pop_front();
    }
}

fn send(session: &LiveSession, chunks: Vec<Vec<i16>>) {
    for c in chunks {
        let _ = session.audio.send(c);
    }
}

async fn run(
    req: LiveRequest,
    connector: Arc<dyn LiveConnector>,
    draft: &(dyn Fn(Option<DraftParts>) + Send + Sync),
    mut rx: mpsc::UnboundedReceiver<Cmd>,
    failed_flag: &AtomicBool,
) -> LiveOutcome {
    let t0 = Instant::now();
    // When the microphone last heard a voice, for the quiet wait at release.
    let voice_rate = req.rate * u32::from(req.channels.max(1));
    let mut last_voice: std::collections::VecDeque<(Instant, u32)> = Default::default();
    let quiet_tail = |log: &std::collections::VecDeque<(Instant, u32)>| {
        log.iter().filter(|(t, _)| t.elapsed() < QUIET_TAIL).map(|(_, ms)| ms).sum::<u32>() < TAIL_VOICE_MS
    };
    let mut chunker = Chunker::new(req.rate, req.channels);
    let mut held: Vec<Vec<i16>> = Vec::new();
    let mut released = false;

    // Recording goes on while the socket opens; its audio is held meanwhile.
    let connect = connector.connect(req.cfg);
    tokio::pin!(connect);
    let session = loop {
        tokio::select! {
            r = &mut connect => match r {
                Ok(s) => break s,
                Err(e) => {
                    failed_flag.store(true, Ordering::SeqCst);
                    wait_for_end(&mut rx, released).await;
                    return LiveOutcome::Fallback { reason: e.to_string(), error: Some(e) };
                }
            },
            cmd = rx.recv(), if !released => match cmd {
                Some(Cmd::Audio(a)) => {
                    heard(&a, voice_rate, &mut last_voice);
                    held.extend(chunker.push(&a));
                }
                Some(Cmd::Release) => released = true,
                Some(Cmd::Cancel) | None => return LiveOutcome::Cancelled,
            },
        }
    };
    send(&session, std::mem::take(&mut held));
    let mut released_at = None;
    if released {
        send(&session, chunker.flush().into_iter().collect());
        session.finish();
        released_at = Some(Instant::now());
    }

    let mut session = session;
    let mut composer = DraftComposer::default();
    let mut throttle = DraftThrottle::default();
    let mut translator = match req.finisher {
        Finisher::Plain => None,
        Finisher::Text { model, system, backup } => {
            let t = SegmentTranslator::new(model, system);
            Some(match backup {
                Some(b) => t.with_backup(b),
                None => t,
            })
        }
    };
    let mut failed: Option<String> = None;
    let mut first_interim = true;
    // Voice sent since the server last said anything (`STALL_AFTER`).
    let mut last_event = Instant::now();
    let mut voice_unanswered: u32 = 0;
    let mut settle_at: Option<Instant> = released_at.filter(|_| quiet_tail(&last_voice)).map(|r| r + QUIET_AT_RELEASE);
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let show = |composer: &DraftComposer, throttle: &mut DraftThrottle| {
        let parts = composer.parts();
        // A word turning white is a change too, even when the text is the same.
        if throttle.due(Instant::now().into_std(), &format!("{}\n{}", parts.settled, parts.interim)) {
            draft(Some(parts));
        }
    };

    loop {
        tokio::select! {
            ev = session.events.recv(), if failed.is_none() => {
                last_event = Instant::now();
                voice_unanswered = 0;
                match ev {
                Some(LiveEvent::Ready) => tracing::info!(setup_ms = t0.elapsed().as_millis() as u64, "timing: live setup_ms"),
                Some(LiveEvent::Interim(t)) => {
                    if first_interim {
                        first_interim = false;
                        tracing::info!(interim_ms = t0.elapsed().as_millis() as u64, "timing: live first interim_ms");
                    }
                    composer.on_interim(&t);
                    settle_at = None;
                    show(&composer, &mut throttle);
                }
                Some(LiveEvent::Segment(t)) => {
                    composer.on_segment(&t);
                    if let Some(tr) = translator.as_mut() {
                        tr.push(t, Instant::now().into_std());
                    }
                    if released_at.is_some() && composer.parts().interim.is_empty() {
                        settle_at = Some(Instant::now() + SETTLE_AFTER_FINAL);
                    }
                    show(&composer, &mut throttle);
                }
                Some(LiveEvent::TurnComplete) if released_at.is_some() => break,
                Some(LiveEvent::TurnComplete) => {}
                Some(LiveEvent::Error(e)) => {
                    failed_flag.store(true, Ordering::SeqCst);
                    failed = Some(e);
                }
                None => {
                    failed_flag.store(true, Ordering::SeqCst);
                    failed = Some("closed".into());
                }
            }},
            cmd = rx.recv(), if released_at.is_none() => match cmd {
                Some(Cmd::Audio(a)) => {
                    heard(&a, voice_rate, &mut last_voice);
                    voice_unanswered += crate::audio::dsp::voice_stats(&a, voice_rate).voiced_ms;
                    if failed.is_none() {
                        send(&session, chunker.push(&a));
                    }
                }
                Some(Cmd::Release) => {
                    if failed.is_some() {
                        break;
                    }
                    send(&session, chunker.flush().into_iter().collect());
                    session.finish();
                    let now = Instant::now();
                    released_at = Some(now);
                    if composer.parts().interim.is_empty() && quiet_tail(&last_voice) {
                        settle_at = Some(now + QUIET_AT_RELEASE);
                    }
                }
                Some(Cmd::Cancel) | None => return LiveOutcome::Cancelled,
            },
            _ = tokio::time::sleep_until(settle_at.unwrap_or_else(Instant::now)), if settle_at.is_some() => break,
            _ = tick.tick() => {
                if failed.is_none() && last_event.elapsed() >= STALL_AFTER && voice_unanswered >= STALL_VOICE_MS {
                    tracing::warn!(voice_ms = voice_unanswered, "live: no answer while speaking; the connection is taken as lost");
                    failed_flag.store(true, Ordering::SeqCst);
                    failed = Some("stalled".into());
                    if released_at.is_some() {
                        break;
                    }
                }
                show(&composer, &mut throttle);
                if let Some(tr) = translator.as_mut() {
                    tr.poll(Instant::now().into_std());
                }
                if released_at.is_some_and(|r| r.elapsed() >= FINAL_WAIT) {
                    break;
                }
            }
        }
    }

    // A connection lost mid-speech: the window keeps its draft, the batch path makes the text.
    if let Some(reason) = failed {
        let error = error_of(&reason);
        return LiveOutcome::Fallback { reason, error: Some(error) };
    }
    if let Some(r) = released_at {
        tracing::info!(final_ms = r.elapsed().as_millis() as u64, "timing: live release→final_ms");
    }
    let raw = composer.final_text();
    if raw.trim().is_empty() {
        return LiveOutcome::Fallback { reason: "empty".into(), error: None };
    }
    let Some(mut tr) = translator else {
        return LiveOutcome::Final { text: raw.clone(), raw, requests: 0 };
    };
    // The last draft without a final piece is the last piece.
    let tail = composer.parts().interim;
    if !tail.is_empty() {
        tr.push(tail, Instant::now().into_std());
    }
    let before = tr.requests();
    match tr.finish().await {
        Ok((text, requests)) if !text.trim().is_empty() => LiveOutcome::Final { text, raw, requests },
        Ok((_, requests)) => LiveOutcome::Unfinished { raw, requests },
        Err(_) => LiveOutcome::Unfinished { raw, requests: before + 1 },
    }
}

/// Before a failed connection is reported, the recording must have ended (the batch path needs
/// all of it).
async fn wait_for_end(rx: &mut mpsc::UnboundedReceiver<Cmd>, released: bool) {
    if released {
        return;
    }
    while let Some(cmd) = rx.recv().await {
        if matches!(cmd, Cmd::Release | Cmd::Cancel) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::gemini_live::TranscribeMode;
    use crate::providers::Completion;
    use std::sync::Mutex;

    /// The server's side of a fake session.
    struct Server {
        audio: mpsc::UnboundedReceiver<Vec<i16>>,
        events: mpsc::UnboundedSender<LiveEvent>,
    }

    struct FakeConnector {
        fail: Option<ProviderError>,
        server: Mutex<Option<Server>>,
    }
    impl FakeConnector {
        fn ok() -> Arc<Self> {
            Arc::new(Self { fail: None, server: Mutex::new(None) })
        }
        fn failing(e: ProviderError) -> Arc<Self> {
            Arc::new(Self { fail: Some(e), server: Mutex::new(None) })
        }
        async fn server(&self) -> Server {
            for _ in 0..100 {
                if let Some(s) = self.server.lock().unwrap().take() {
                    return s;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            panic!("never connected");
        }
    }
    #[async_trait::async_trait]
    impl LiveConnector for FakeConnector {
        async fn connect(&self, _: LiveConfig) -> Result<LiveSession, ProviderError> {
            if let Some(e) = &self.fail {
                return Err(e.clone());
            }
            let (audio_tx, audio_rx) = mpsc::unbounded_channel();
            let (ev_tx, ev_rx) = mpsc::unbounded_channel();
            *self.server.lock().unwrap() = Some(Server { audio: audio_rx, events: ev_tx });
            Ok(LiveSession { audio: audio_tx, events: ev_rx })
        }
    }

    struct FakeModel {
        reply: String,
        delay: Duration,
        calls: Mutex<u32>,
    }
    impl FakeModel {
        fn new(reply: &str, delay_ms: u64) -> Arc<Self> {
            Arc::new(Self { reply: reply.into(), delay: Duration::from_millis(delay_ms), calls: Mutex::new(0) })
        }
    }
    #[async_trait::async_trait]
    impl TextModel for FakeModel {
        async fn complete(&self, _: &str, _: &str) -> Result<Completion, ProviderError> {
            *self.calls.lock().unwrap() += 1;
            tokio::time::sleep(self.delay).await;
            Ok(Completion { text: self.reply.clone(), total_tokens: 1 })
        }
    }

    type Drafts = Arc<Mutex<Vec<(Instant, Option<DraftParts>)>>>;

    struct Run {
        handle: LiveHandle,
        drafts: Drafts,
        outcome: tokio::sync::oneshot::Receiver<LiveOutcome>,
    }

    fn cfg() -> LiveConfig {
        LiveConfig { url: "ws://fake".into(), key: "k".into(), model: "m".into(), mode: TranscribeMode::Verbatim, language: Some("ru".into()) }
    }

    fn start(connector: Arc<dyn LiveConnector>, finisher: Finisher) -> Run {
        let drafts: Drafts = Default::default();
        let d = drafts.clone();
        let (tx, outcome) = tokio::sync::oneshot::channel();
        let req = LiveRequest { cfg: cfg(), finisher, rate: 16_000, channels: 1 };
        let handle = spawn(req, connector, Arc::new(move |p| d.lock().unwrap().push((Instant::now(), p))), move |o| {
            let _ = tx.send(o);
        });
        Run { handle, drafts, outcome }
    }

    fn say(s: &Server, ev: LiveEvent) {
        s.events.send(ev).unwrap();
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    struct PanicConnector;
    #[async_trait::async_trait]
    impl LiveConnector for PanicConnector {
        async fn connect(&self, _: LiveConfig) -> Result<LiveSession, ProviderError> {
            panic!("simulated bug in a live dictation");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn crash_in_runner_falls_back() {
        let r = start(Arc::new(PanicConnector), Finisher::Plain);
        r.handle.release();
        let out = r.outcome.await.unwrap();
        assert!(matches!(&out, LiveOutcome::Fallback { reason, .. } if reason == "crashed"), "{out:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn plain_dictation_returns_final() {
        let c = FakeConnector::ok();
        let r = start(c.clone(), Finisher::Plain);
        let s = c.server().await;
        say(&s, LiveEvent::Ready);
        say(&s, LiveEvent::Interim("привет".into()));
        settle().await;
        say(&s, LiveEvent::Interim("привет мир".into()));
        tokio::time::sleep(Duration::from_millis(150)).await;
        say(&s, LiveEvent::Segment("Привет, мир.".into()));
        settle().await;
        r.handle.release();
        settle().await;
        say(&s, LiveEvent::TurnComplete);
        let out = r.outcome.await.unwrap();
        assert_eq!(out, LiveOutcome::Final { text: "Привет, мир.".into(), raw: "Привет, мир.".into(), requests: 0 });
        let drafts = r.drafts.lock().unwrap().clone();
        assert!(drafts.iter().all(|(_, p)| p.is_some()), "the orchestrator clears the window after pasting");
        let shown: Vec<_> = drafts.iter().filter(|(_, p)| p.is_some()).collect();
        assert!(!shown.is_empty());
        for w in shown.windows(2) {
            assert!(w[1].0 - w[0].0 >= Duration::from_millis(100), "at most every 100 ms");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn final_piece_after_release_ends_without_turn_complete() {
        // Google sends the last final piece ~0.4 s after the stream ends, but no turnComplete.
        let c = FakeConnector::ok();
        let r = start(c.clone(), Finisher::Plain);
        let s = c.server().await;
        say(&s, LiveEvent::Interim("привет мир".into()));
        settle().await;
        r.handle.release();
        tokio::time::sleep(Duration::from_millis(400)).await;
        let t = Instant::now();
        say(&s, LiveEvent::Segment("Привет, мир.".into()));
        let out = r.outcome.await.unwrap();
        assert_eq!(out, LiveOutcome::Final { text: "Привет, мир.".into(), raw: "Привет, мир.".into(), requests: 0 });
        assert!(t.elapsed() <= SETTLE_AFTER_FINAL + Duration::from_millis(20), "{:?}", t.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_pending_at_release_ends_soon() {
        // A pause before letting go: Google finalised everything already and sends nothing more.
        let c = FakeConnector::ok();
        let r = start(c.clone(), Finisher::Plain);
        let s = c.server().await;
        say(&s, LiveEvent::Interim("всё сказано".into()));
        say(&s, LiveEvent::Segment("Всё сказано.".into()));
        settle().await;
        let t = Instant::now();
        r.handle.release();
        let LiveOutcome::Final { text, .. } = r.outcome.await.unwrap() else { panic!() };
        assert_eq!(text, "Всё сказано.");
        assert!(t.elapsed() <= QUIET_AT_RELEASE + Duration::from_millis(20), "{:?}", t.elapsed());
    }

    /// 16 kHz mono: `ms` of a loud tone (speech for the voice check) or of silence.
    fn sound(ms: u64, loud: bool) -> Vec<f32> {
        (0..16 * ms as usize).map(|i| if loud { 0.3 * ((i as f32) * 0.2).sin() } else { 0.0 }).collect()
    }

    #[tokio::test(start_paused = true)]
    async fn words_spoken_right_before_release_are_waited_for() {
        // Review 2026-11-07: Google had finalised what it heard, but the user said two more words
        // right before letting go; their piece came 0.6 s after the release, after the 0.5 s quiet
        // wait — the words were lost.
        let c = FakeConnector::ok();
        let r = start(c.clone(), Finisher::Plain);
        let s = c.server().await;
        say(&s, LiveEvent::Segment("Начало.".into()));
        settle().await;
        r.handle.audio(&sound(300, true));
        settle().await;
        r.handle.release();
        tokio::time::sleep(Duration::from_millis(600)).await;
        say(&s, LiveEvent::Segment("И конец.".into()));
        let LiveOutcome::Final { text, .. } = r.outcome.await.unwrap() else { panic!() };
        assert_eq!(text, "Начало. И конец.");
    }

    #[tokio::test(start_paused = true)]
    async fn silence_before_release_still_ends_soon() {
        let c = FakeConnector::ok();
        let r = start(c.clone(), Finisher::Plain);
        let s = c.server().await;
        r.handle.audio(&sound(300, true));
        settle().await;
        tokio::time::sleep(Duration::from_millis(800)).await;
        say(&s, LiveEvent::Segment("Всё сказано.".into()));
        r.handle.audio(&sound(800, false));
        settle().await;
        let t = Instant::now();
        r.handle.release();
        let LiveOutcome::Final { text, .. } = r.outcome.await.unwrap() else { panic!() };
        assert_eq!(text, "Всё сказано.");
        assert!(t.elapsed() <= QUIET_AT_RELEASE + Duration::from_millis(20), "{:?}", t.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn a_click_before_release_does_not_hold_the_paste() {
        // Clicks and breaths are loud for a moment; they must not cost the quick end.
        let c = FakeConnector::ok();
        let r = start(c.clone(), Finisher::Plain);
        let s = c.server().await;
        say(&s, LiveEvent::Segment("Всё сказано.".into()));
        settle().await;
        let mut click = sound(40, true);
        click.extend(sound(200, false));
        r.handle.audio(&click);
        settle().await;
        let t = Instant::now();
        r.handle.release();
        let LiveOutcome::Final { text, .. } = r.outcome.await.unwrap() else { panic!() };
        assert_eq!(text, "Всё сказано.");
        assert!(t.elapsed() <= QUIET_AT_RELEASE + Duration::from_millis(20), "{:?}", t.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn a_silent_server_while_the_user_speaks_counts_as_a_failed_connection() {
        // Review 2026-11-07: a connection that dies without an error sends nothing more; the live
        // text would end where the server stopped hearing. The recording takes the batch path.
        let c = FakeConnector::ok();
        let r = start(c.clone(), Finisher::Plain);
        let s = c.server().await;
        say(&s, LiveEvent::Interim("начало".into()));
        settle().await;
        for _ in 0..25 {
            r.handle.audio(&sound(100, true));
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(!r.handle.failed(), "2.5 s without an answer is still normal");
        for _ in 0..10 {
            r.handle.audio(&sound(100, true));
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(r.handle.failed(), "3.5 s of speech without a word back");
        r.handle.release();
        let out = r.outcome.await.unwrap();
        assert!(matches!(&out, LiveOutcome::Fallback { reason, .. } if reason == "stalled"), "{out:?}");
        drop(s);
    }

    #[tokio::test(start_paused = true)]
    async fn a_silent_server_during_a_pause_is_fine() {
        let c = FakeConnector::ok();
        let r = start(c.clone(), Finisher::Plain);
        let s = c.server().await;
        say(&s, LiveEvent::Segment("Начало.".into()));
        settle().await;
        for _ in 0..50 {
            r.handle.audio(&sound(100, false));
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(!r.handle.failed());
        drop(s);
    }

    #[tokio::test(start_paused = true)]
    async fn late_words_after_quiet_release_still_arrive() {
        let c = FakeConnector::ok();
        let r = start(c.clone(), Finisher::Plain);
        let s = c.server().await;
        say(&s, LiveEvent::Segment("Начало.".into()));
        settle().await;
        r.handle.release();
        tokio::time::sleep(Duration::from_millis(200)).await;
        say(&s, LiveEvent::Interim("и конец".into()));
        tokio::time::sleep(Duration::from_millis(500)).await;
        say(&s, LiveEvent::Segment("И конец.".into()));
        let LiveOutcome::Final { text, .. } = r.outcome.await.unwrap() else { panic!() };
        assert_eq!(text, "Начало. И конец.");
    }

    #[tokio::test(start_paused = true)]
    async fn new_draft_after_final_piece_keeps_waiting() {
        let c = FakeConnector::ok();
        let r = start(c.clone(), Finisher::Plain);
        let s = c.server().await;
        r.handle.release();
        settle().await;
        say(&s, LiveEvent::Segment("Первое.".into()));
        tokio::time::sleep(Duration::from_millis(50)).await;
        say(&s, LiveEvent::Interim("второе".into()));
        tokio::time::sleep(Duration::from_millis(300)).await;
        say(&s, LiveEvent::Segment("Второе.".into()));
        let LiveOutcome::Final { text, .. } = r.outcome.await.unwrap() else { panic!() };
        assert_eq!(text, "Первое. Второе.");
    }

    #[tokio::test(start_paused = true)]
    async fn clean_dictation_runs_text_model() {
        let c = FakeConnector::ok();
        let model = FakeModel::new("X.", 100);
        let r = start(c.clone(), Finisher::Text { model: model.clone(), system: "clean".into(), backup: None });
        let s = c.server().await;
        say(&s, LiveEvent::Segment("x".into()));
        settle().await;
        r.handle.release();
        settle().await;
        say(&s, LiveEvent::TurnComplete);
        let out = r.outcome.await.unwrap();
        let LiveOutcome::Final { text, raw, requests } = out else { panic!("{out:?}") };
        assert_eq!((text.as_str(), raw.as_str()), ("X.", "x"));
        assert!(requests >= 1);
    }

    #[tokio::test(start_paused = true)]
    async fn translate_returns_model_answer() {
        let c = FakeConnector::ok();
        let r = start(c.clone(), Finisher::Text { model: FakeModel::new("Hello.", 50), system: "translate".into(), backup: None });
        let s = c.server().await;
        say(&s, LiveEvent::Segment("Привет.".into()));
        settle().await;
        r.handle.release();
        settle().await;
        say(&s, LiveEvent::TurnComplete);
        let LiveOutcome::Final { text, .. } = r.outcome.await.unwrap() else { panic!() };
        assert_eq!(text, "Hello.");
    }

    #[tokio::test(start_paused = true)]
    async fn tail_without_segment_is_kept() {
        let c = FakeConnector::ok();
        let r = start(c.clone(), Finisher::Plain);
        let s = c.server().await;
        say(&s, LiveEvent::Segment("Начало.".into()));
        say(&s, LiveEvent::Interim("и хвост".into()));
        settle().await;
        r.handle.release();
        let t = Instant::now();
        let LiveOutcome::Final { text, .. } = r.outcome.await.unwrap() else { panic!() };
        assert_eq!(text, "Начало. и хвост");
        assert!(t.elapsed() <= FINAL_WAIT + Duration::from_millis(150), "{:?}", t.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn connect_failure_falls_back() {
        let r = start(FakeConnector::failing(ProviderError::Network), Finisher::Plain);
        r.handle.audio(&[0.1; 1_600]);
        r.handle.release();
        let out = r.outcome.await.unwrap();
        assert!(matches!(out, LiveOutcome::Fallback { error: Some(ProviderError::Network), .. }), "{out:?}");
        assert!(r.drafts.lock().unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn silent_server_falls_back() {
        let c = FakeConnector::ok();
        let r = start(c.clone(), Finisher::Plain);
        let _s = c.server().await;
        r.handle.release();
        let t = Instant::now();
        let out = r.outcome.await.unwrap();
        assert_eq!(out, LiveOutcome::Fallback { reason: "empty".into(), error: None });
        assert!(t.elapsed() <= FINAL_WAIT + Duration::from_millis(150));
    }

    #[tokio::test(start_paused = true)]
    async fn server_error_mid_speech_falls_back() {
        let c = FakeConnector::ok();
        let mut r = start(c.clone(), Finisher::Plain);
        let s = c.server().await;
        say(&s, LiveEvent::Interim("начало".into()));
        say(&s, LiveEvent::Error("User location is not supported for the API use.".into()));
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(r.outcome.try_recv().is_err(), "nothing until the user lets go");
        r.handle.release();
        let out = r.outcome.await.unwrap();
        assert!(matches!(&out, LiveOutcome::Fallback { reason, error: Some(ProviderError::BadResponse(_)) } if reason.contains("location")), "{out:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn text_model_timeout_is_unfinished() {
        let c = FakeConnector::ok();
        let r = start(c.clone(), Finisher::Text { model: FakeModel::new("never", 10_000), system: "clean".into(), backup: None });
        let s = c.server().await;
        say(&s, LiveEvent::Segment("сырой текст".into()));
        settle().await;
        r.handle.release();
        settle().await;
        say(&s, LiveEvent::TurnComplete);
        let t = Instant::now();
        let out = r.outcome.await.unwrap();
        assert!(matches!(&out, LiveOutcome::Unfinished { raw, .. } if raw == "сырой текст"), "{out:?}");
        assert!(t.elapsed() <= Duration::from_millis(3_100), "{:?}", t.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn cancel_sends_nothing() {
        let c = FakeConnector::ok();
        let model = FakeModel::new("x", 0);
        let r = start(c.clone(), Finisher::Text { model: model.clone(), system: "clean".into(), backup: None });
        let s = c.server().await;
        say(&s, LiveEvent::Interim("отмена".into()));
        settle().await;
        r.handle.cancel();
        assert_eq!(r.outcome.await.unwrap(), LiveOutcome::Cancelled);
        assert_eq!(*model.calls.lock().unwrap(), 0);
        assert_eq!(r.drafts.lock().unwrap().last().unwrap().1, None);
    }

    #[tokio::test(start_paused = true)]
    async fn audio_is_chunked_and_streamed() {
        let c = FakeConnector::ok();
        let drafts: Drafts = Default::default();
        let d = drafts.clone();
        let (tx, outcome) = tokio::sync::oneshot::channel();
        let req = LiveRequest { cfg: cfg(), finisher: Finisher::Plain, rate: 48_000, channels: 2 };
        let handle = spawn(req, c.clone(), Arc::new(move |p| d.lock().unwrap().push((Instant::now(), p))), move |o| {
            let _ = tx.send(o);
        });
        // Audio before the socket is open is held, then sent in order.
        handle.audio(&vec![0.2; 48_000 * 2 * 35 / 100]);
        let mut s = c.server().await;
        settle().await;
        handle.release();
        settle().await;
        let mut sizes = Vec::new();
        while let Ok(chunk) = s.audio.try_recv() {
            sizes.push(chunk.len());
        }
        assert_eq!(sizes.iter().sum::<usize>(), 5_600, "{sizes:?}");
        assert!(sizes[..sizes.len() - 2].iter().all(|&n| n == 1_600), "{sizes:?}");
        assert_eq!(sizes.last(), Some(&0), "the stream end follows the audio");
        drop(outcome);
    }
}
