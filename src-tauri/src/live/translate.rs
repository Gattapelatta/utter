//! Translation of final pieces while the user speaks (spec 2026-11-04 §4.2, §5.6).
//! Pieces are joined into one request at most every 4 s: free tiers allow 5–15 requests
//! a minute and a long dictation has 30+ pieces.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::oneshot;

use crate::pipeline::{postprocess, user_message};
use crate::providers::{ProviderError, TextModel};

/// At most one translation request starts per this interval while speaking.
pub const TRANSLATE_EVERY: Duration = Duration::from_secs(4);
/// A request without an answer this long is dropped; its pieces go with the next one.
pub const TRANSLATE_TIMEOUT: Duration = Duration::from_secs(3);

type Reply = Result<String, ProviderError>;

struct InFlight {
    pieces: Range<usize>,
    started: Instant,
    task: tokio::task::JoinHandle<()>,
    reply: oneshot::Receiver<Reply>,
}

pub struct SegmentTranslator {
    provider: Arc<dyn TextModel>,
    system: String,
    segments: Vec<String>,
    /// Translations by the first piece they cover: (end of the range, text).
    done: BTreeMap<usize, (usize, String)>,
    /// First piece not translated and not in flight.
    next: usize,
    in_flight: Option<InFlight>,
    last_start: Option<Instant>,
    requests: u32,
}

impl SegmentTranslator {
    pub fn new(provider: Arc<dyn TextModel>, system: String) -> Self {
        Self {
            provider,
            system,
            segments: Vec::new(),
            done: BTreeMap::new(),
            next: 0,
            in_flight: None,
            last_start: None,
            requests: 0,
        }
    }

    pub fn push(&mut self, segment: String, now: Instant) {
        self.segments.push(segment);
        self.poll(now);
    }

    /// Requests started so far (each counts in usage).
    pub fn requests(&self) -> u32 {
        self.requests
    }

    fn start(&mut self, pieces: Range<usize>, now: Instant) -> InFlight {
        let text = self.segments[pieces.clone()].iter().map(|s| s.trim()).collect::<Vec<_>>().join(" ");
        let (tx, reply) = oneshot::channel();
        let (provider, system) = (self.provider.clone(), self.system.clone());
        let task = tokio::spawn(async move {
            let r = provider.complete(&system, &user_message(&text)).await.map(|c| postprocess(&c.text));
            let _ = tx.send(r);
        });
        self.requests += 1;
        self.last_start = Some(now);
        InFlight { pieces, started: now, task, reply }
    }

    /// Collects a finished request, drops a stuck one, starts the next batch when allowed.
    pub fn poll(&mut self, now: Instant) {
        if let Some(mut f) = self.in_flight.take() {
            match f.reply.try_recv() {
                Ok(Ok(text)) => {
                    self.done.insert(f.pieces.start, (f.pieces.end, text));
                }
                // A failed request: its pieces go again with the next one.
                Ok(Err(_)) | Err(oneshot::error::TryRecvError::Closed) => self.next = f.pieces.start,
                Err(oneshot::error::TryRecvError::Empty) if now.saturating_duration_since(f.started) >= TRANSLATE_TIMEOUT => {
                    f.task.abort();
                    self.next = f.pieces.start;
                }
                Err(oneshot::error::TryRecvError::Empty) => self.in_flight = Some(f),
            }
        }
        let spaced = self.last_start.is_none_or(|t| now.saturating_duration_since(t) >= TRANSLATE_EVERY);
        if self.in_flight.is_none() && self.next < self.segments.len() && spaced {
            let pieces = self.next..self.segments.len();
            self.next = pieces.end;
            self.in_flight = Some(self.start(pieces, now));
        }
    }

    /// Translates the rest at once (alongside a request still in flight), waits at most
    /// `TRANSLATE_TIMEOUT`, and joins the translations in piece order.
    pub async fn finish(mut self) -> Result<(String, u32), ProviderError> {
        self.poll(Instant::now());
        let mut waiting = Vec::new();
        if let Some(f) = self.in_flight.take() {
            waiting.push(f);
        }
        if self.next < self.segments.len() {
            let pieces = self.next..self.segments.len();
            self.next = pieces.end;
            let f = self.start(pieces, Instant::now());
            waiting.push(f);
        }
        let deadline = tokio::time::Instant::now() + TRANSLATE_TIMEOUT;
        for f in waiting {
            let reply = tokio::time::timeout_at(deadline, f.reply).await;
            match reply {
                Ok(Ok(Ok(text))) => {
                    self.done.insert(f.pieces.start, (f.pieces.end, text));
                }
                Ok(Ok(Err(e))) => return Err(e),
                Ok(Err(_)) => return Err(ProviderError::BadResponse("translation task crashed".into())),
                Err(_) => {
                    f.task.abort();
                    return Err(ProviderError::Timeout);
                }
            }
        }
        let text = self.done.values().map(|(_, t)| t.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
        Ok((text, self.requests))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::Completion;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// Replies `EN[<transcript>]` after the next delay in the queue (or 100 ms).
    #[derive(Default)]
    struct FakeTranslator {
        delays: Mutex<VecDeque<Duration>>,
        calls: Mutex<Vec<(tokio::time::Instant, String)>>,
    }
    impl FakeTranslator {
        fn with_delays(d: &[u64]) -> Arc<Self> {
            let f = Self::default();
            *f.delays.lock().unwrap() = d.iter().map(|&ms| Duration::from_millis(ms)).collect();
            Arc::new(f)
        }
        fn inputs(&self) -> Vec<String> {
            self.calls.lock().unwrap().iter().map(|(_, t)| t.clone()).collect()
        }
    }
    #[async_trait::async_trait]
    impl TextModel for FakeTranslator {
        async fn complete(&self, _: &str, user: &str) -> Result<Completion, ProviderError> {
            let text = user.trim_start_matches("<transcript>\n").trim_end_matches("\n</transcript>").to_string();
            self.calls.lock().unwrap().push((tokio::time::Instant::now(), text.clone()));
            let delay = self.delays.lock().unwrap().pop_front().unwrap_or(Duration::from_millis(100));
            tokio::time::sleep(delay).await;
            Ok(Completion { text: format!("EN[{text}]"), total_tokens: 1 })
        }
    }

    /// Lets spawned requests run for `ms` of (paused) tokio time.
    async fn run_for(ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    #[tokio::test(start_paused = true)]
    async fn translations_join_in_order() {
        let fake = FakeTranslator::with_delays(&[2_000, 10]);
        let mut t = SegmentTranslator::new(fake.clone(), "sys".into());
        let t0 = Instant::now();
        t.push("первый".into(), t0);
        run_for(1).await;
        t.push("второй".into(), t0 + Duration::from_secs(1));
        // The second request (started by finish) answers long before the first.
        let (text, requests) = t.finish().await.unwrap();
        assert_eq!(text, "EN[первый] EN[второй]");
        assert_eq!(requests, 2);
    }

    #[tokio::test(start_paused = true)]
    async fn translation_coalesces_segments() {
        let fake = FakeTranslator::with_delays(&[]);
        let mut t = SegmentTranslator::new(fake.clone(), "sys".into());
        let t0 = Instant::now();
        for i in 0..10u64 {
            t.push(format!("кусок{i}"), t0 + Duration::from_secs(2 * i));
            run_for(2_000).await;
            t.poll(t0 + Duration::from_secs(2 * i + 2));
        }
        let (text, requests) = t.finish().await.unwrap();
        assert!(requests <= 6, "{requests} requests");
        let calls = fake.calls.lock().unwrap().clone();
        for w in calls.windows(2) {
            assert!(w[1].0 - w[0].0 >= TRANSLATE_EVERY, "{:?}", w[1].0 - w[0].0);
        }
        for i in 0..10 {
            assert!(text.contains(&format!("кусок{i}")), "{text}");
        }
        let pos: Vec<usize> = (0..10).map(|i| text.find(&format!("кусок{i}")).unwrap()).collect();
        assert!(pos.windows(2).all(|w| w[0] < w[1]), "{text}");
    }

    #[tokio::test(start_paused = true)]
    async fn finish_translates_rest_at_once() {
        let fake = FakeTranslator::with_delays(&[]);
        let mut t = SegmentTranslator::new(fake.clone(), "sys".into());
        let t0 = Instant::now();
        t.push("один".into(), t0);
        run_for(200).await;
        t.push("два".into(), t0 + Duration::from_millis(200));
        t.push("три".into(), t0 + Duration::from_millis(300));
        let started = tokio::time::Instant::now();
        let (text, _) = t.finish().await.unwrap();
        assert_eq!(text, "EN[один] EN[два три]");
        assert!(started.elapsed() < Duration::from_millis(500), "no 4 s wait at the end");
        assert_eq!(fake.inputs(), vec!["один".to_string(), "два три".to_string()]);
    }

    #[tokio::test(start_paused = true)]
    async fn finish_gives_up_after_3s() {
        let fake = FakeTranslator::with_delays(&[10_000]);
        let mut t = SegmentTranslator::new(fake, "sys".into());
        t.push("долго".into(), Instant::now());
        let started = tokio::time::Instant::now();
        assert_eq!(t.finish().await, Err(ProviderError::Timeout));
        assert_eq!(started.elapsed(), TRANSLATE_TIMEOUT);
    }

    #[tokio::test(start_paused = true)]
    async fn slow_translation_is_retried_with_next() {
        let fake = FakeTranslator::with_delays(&[10_000]);
        let mut t = SegmentTranslator::new(fake.clone(), "sys".into());
        let t0 = Instant::now();
        t.push("первый".into(), t0);
        run_for(1).await;
        t.push("второй".into(), t0 + Duration::from_secs(1));
        t.poll(t0 + Duration::from_millis(3_100));
        t.poll(t0 + Duration::from_secs(4));
        run_for(200).await;
        let (text, requests) = t.finish().await.unwrap();
        assert_eq!(text, "EN[первый второй]");
        assert_eq!(requests, 2);
        assert_eq!(fake.inputs(), vec!["первый".to_string(), "первый второй".to_string()]);
    }

    #[tokio::test]
    async fn nothing_to_translate_is_empty() {
        let t = SegmentTranslator::new(FakeTranslator::with_delays(&[]), "sys".into());
        assert_eq!(t.finish().await, Ok((String::new(), 0)));
    }
}
