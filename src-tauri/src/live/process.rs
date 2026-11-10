//! Cleanup or translation of the final pieces while the user speaks (spec 2026-11-10 §4–5):
//! every piece goes at once, requests run in parallel within the shared budget, a silent one
//! is raced after `RETRY_AFTER`.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::time::Instant;

use crate::pipeline::fillers::strip_fillers;
use crate::pipeline::routes::RouteHealth;
use crate::pipeline::{normalize, postprocess, user_message};
use crate::providers::budget::{Priority, SharedBudget};
use crate::providers::defaults::BACKUP_LITE_ROUTE;
use crate::providers::{ProviderError, TextModel};

/// A request without an answer this long while speaking is sent once more (the backup Lite
/// model while healthy); a backup silent this long is taken as sick.
pub const RETRY_AFTER: Duration = Duration::from_millis(2_500);
/// Attempts one request may have in all; past them it waits for the ones still running.
const MAX_ATTEMPTS: usize = 4;
/// After release a translation still unanswered this long gets a fresh request to the main
/// model (spec §4.3).
const TRANSLATE_LADDER: Duration = Duration::from_secs(1);

pub struct Models {
    pub main: Arc<dyn TextModel>,
    pub main_id: String,
    pub backup: Option<Arc<dyn TextModel>>,
    pub backup_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Cleanup,
    Translate,
}

type Reply = Result<String, ProviderError>;

struct Attempt {
    backup: bool,
    sent: Instant,
    task: tokio::task::JoinHandle<()>,
    reply: oneshot::Receiver<Reply>,
    /// Answered with an error, or its channel closed.
    over: bool,
    /// A slow backup is punished once.
    judged: bool,
}

/// One request text and its attempts.
struct Job {
    pieces: Range<usize>,
    text: String,
    attempts: Vec<Attempt>,
    first_sent: Instant,
    last_sent: Instant,
}

/// The tail sent ahead (spec §4.1 p. 4): becomes its piece's request when the final piece
/// has the same words.
struct Guess {
    key: String,
    job: Job,
    answer: Option<String>,
}

/// What one look at a request's attempts found.
#[derive(Default)]
struct Look {
    answer: Option<String>,
    errors: Vec<(bool, ProviderError)>,
    slow_backup: bool,
}

fn look(attempts: &mut [Attempt], now: Instant) -> Look {
    let mut l = Look::default();
    for a in attempts.iter_mut().filter(|a| !a.over) {
        match a.reply.try_recv() {
            Ok(Ok(text)) => {
                l.answer = Some(text);
                break;
            }
            Ok(Err(e)) => {
                a.over = true;
                l.errors.push((a.backup, e));
            }
            Err(oneshot::error::TryRecvError::Closed) => a.over = true,
            Err(oneshot::error::TryRecvError::Empty) => {
                if a.backup && !a.judged && now.saturating_duration_since(a.sent) >= RETRY_AFTER {
                    a.judged = true;
                    l.slow_backup = true;
                }
            }
        }
    }
    l
}

fn abort(attempts: &[Attempt]) {
    for a in attempts {
        a.task.abort();
    }
}

/// The text to paste and how it was made.
#[derive(Debug, PartialEq)]
pub struct Ends {
    pub text: String,
    pub pieces: usize,
    /// Pieces pasted as heard (no answer by the deadline).
    pub raw_pieces: usize,
    pub requests: u32,
}

pub struct PieceProcessor {
    models: Models,
    system: String,
    kind: Kind,
    budget: SharedBudget,
    health: Arc<Mutex<RouteHealth>>,
    /// Verbatim pieces, in order.
    segments: Vec<String>,
    /// Pieces already given to a job (or answered).
    assigned: Vec<bool>,
    jobs: Vec<Job>,
    /// Answers by the first piece they cover: (end of the range, text).
    done: BTreeMap<usize, (usize, String)>,
    guess: Option<Guess>,
    released: bool,
    requests: u32,
}

impl PieceProcessor {
    pub fn new(models: Models, system: String, kind: Kind, budget: SharedBudget, health: Arc<Mutex<RouteHealth>>) -> Self {
        Self {
            models,
            system,
            kind,
            budget,
            health,
            segments: Vec::new(),
            assigned: Vec::new(),
            jobs: Vec::new(),
            done: BTreeMap::new(),
            guess: None,
            released: false,
            requests: 0,
        }
    }

    /// A final piece from the recogniser.
    pub fn push(&mut self, segment: String) {
        let i = self.segments.len();
        let key = normalize(&segment);
        self.segments.push(segment);
        self.assigned.push(false);
        if let Some(g) = self.guess.take() {
            let hit = g.key == key;
            tracing::info!(hit, "timing: live guess");
            if hit {
                self.assigned[i] = true;
                match g.answer {
                    Some(text) => {
                        self.done.insert(i, (i + 1, text));
                    }
                    None => self.jobs.push(Job { pieces: i..i + 1, ..g.job }),
                }
            } else {
                abort(&g.job.attempts);
            }
        }
        self.poll();
    }

    fn priority(&self) -> Priority {
        if self.released {
            Priority::End
        } else {
            Priority::Speech
        }
    }

    fn retry_every(&self) -> Duration {
        if self.released && self.kind == Kind::Translate {
            TRANSLATE_LADDER
        } else {
            RETRY_AFTER
        }
    }

    /// One more attempt for a request: the backup while healthy and not tried yet, else
    /// a fresh one to the main model.
    fn one_more(&mut self, backup_tried: bool, text: &str, p: Priority) -> Option<Attempt> {
        let try_backup = !backup_tried && self.backup_healthy();
        self.attempt(text, try_backup, p).or_else(|| if try_backup { self.attempt(text, false, p) } else { None })
    }

    fn backup_healthy(&self) -> bool {
        self.models.backup.is_some() && self.health.lock().is_ok_and(|h| h.healthy(BACKUP_LITE_ROUTE, Instant::now().into_std()))
    }

    fn punish_backup(&self) {
        if let Ok(mut h) = self.health.lock() {
            h.punish(BACKUP_LITE_ROUTE, Instant::now().into_std());
        }
    }

    /// Starts one attempt of `text` on the main or the backup model if the budget allows.
    fn attempt(&mut self, text: &str, backup: bool, p: Priority) -> Option<Attempt> {
        let (model, id) = if backup {
            (self.models.backup.clone()?, self.models.backup_id.as_str())
        } else {
            (self.models.main.clone(), self.models.main_id.as_str())
        };
        let now = Instant::now();
        if !self.budget.lock().is_ok_and(|mut b| b.try_take(id, p, now)) {
            return None;
        }
        let (tx, reply) = oneshot::channel();
        let (system, user) = (self.system.clone(), user_message(text));
        let task = tokio::spawn(async move {
            let r = model.complete(&system, &user).await.map(|c| postprocess(&c.text));
            let _ = tx.send(r);
        });
        self.requests += 1;
        Some(Attempt { backup, sent: now, task, reply, over: false, judged: false })
    }

    /// Collects answers, races silent requests, sends waiting pieces when the budget allows.
    pub fn poll(&mut self) {
        self.collect();
        self.race();
        self.send_waiting();
    }

    fn collect(&mut self) {
        let now = Instant::now();
        let mut answered: Vec<(usize, String)> = Vec::new();
        let mut errors: Vec<(bool, ProviderError)> = Vec::new();
        let mut slow_backup = false;
        for (ji, job) in self.jobs.iter_mut().enumerate() {
            let l = look(&mut job.attempts, now);
            errors.extend(l.errors);
            slow_backup |= l.slow_backup;
            if let Some(text) = l.answer {
                answered.push((ji, text));
            }
        }
        if let Some(g) = self.guess.as_mut().filter(|g| g.answer.is_none()) {
            let l = look(&mut g.job.attempts, now);
            errors.extend(l.errors);
            slow_backup |= l.slow_backup;
            if l.answer.is_some() {
                abort(&g.job.attempts);
                g.answer = l.answer;
            }
        }
        for (backup, e) in errors {
            if backup {
                slow_backup = true;
            }
            if let ProviderError::RateLimited { retry_after } = e {
                let id = if backup { &self.models.backup_id } else { &self.models.main_id };
                if let Ok(mut b) = self.budget.lock() {
                    b.rate_limited(id, retry_after, now);
                }
            }
        }
        if slow_backup {
            self.punish_backup();
        }
        for (ji, text) in answered.into_iter().rev() {
            let job = self.jobs.remove(ji);
            abort(&job.attempts);
            tracing::info!(
                piece_ms = now.saturating_duration_since(job.first_sent).as_millis() as u64,
                attempts = job.attempts.len(),
                chars = job.text.chars().count(),
                "timing: live piece processed"
            );
            self.done.insert(job.pieces.start, (job.pieces.end, text));
        }
    }

    /// A request with no attempt left running, or silent `RETRY_AFTER` since its last one,
    /// gets one more: the backup while healthy and not tried yet, else the main model.
    fn race(&mut self) {
        let now = Instant::now();
        let (p, every) = (self.priority(), self.retry_every());
        let due = |j: &Job| {
            let running = j.attempts.iter().filter(|a| !a.over).count();
            (running == 0 || now.saturating_duration_since(j.last_sent) >= every) && j.attempts.len() < MAX_ATTEMPTS
        };
        for ji in 0..self.jobs.len() {
            let job = &self.jobs[ji];
            if !due(job) {
                continue;
            }
            let (tried, text) = (job.attempts.iter().any(|a| a.backup), job.text.clone());
            if let Some(a) = self.one_more(tried, &text, p) {
                let job = &mut self.jobs[ji];
                tracing::info!(attempt = job.attempts.len() + 1, waited_ms = now.saturating_duration_since(job.first_sent).as_millis() as u64, "timing: live text sent again");
                job.attempts.push(a);
                job.last_sent = now;
            }
        }
        // After release the tail is a request like the others.
        let guess = self.guess.as_ref().filter(|g| self.released && g.answer.is_none() && due(&g.job));
        if let Some((tried, text)) = guess.map(|g| (g.job.attempts.iter().any(|a| a.backup), g.job.text.clone())) {
            if let Some(a) = self.one_more(tried, &text, p) {
                let g = self.guess.as_mut().expect("checked");
                g.job.attempts.push(a);
                g.job.last_sent = now;
            }
        }
    }

    /// Pieces not in any request go as one request per run of neighbours.
    fn send_waiting(&mut self) {
        let p = self.priority();
        let mut i = 0;
        while i < self.segments.len() {
            if self.assigned[i] {
                i += 1;
                continue;
            }
            let start = i;
            while i < self.segments.len() && !self.assigned[i] {
                i += 1;
            }
            let pieces = start..i;
            let text = self.joined(&pieces);
            let Some(a) = self.attempt(&text, false, p) else { return };
            let mut attempts = vec![a];
            // After release: two attempts at once (spec §4.2 p. 2).
            if self.released {
                attempts.extend(self.one_more(false, &text, p));
            }
            self.assigned[pieces.clone()].iter_mut().for_each(|x| *x = true);
            let now = Instant::now();
            self.jobs.push(Job { pieces, text, attempts, first_sent: now, last_sent: now });
        }
    }

    fn joined(&self, pieces: &Range<usize>) -> String {
        self.segments[pieces.clone()].iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ")
    }

    /// Sends the tail (the draft of the last piece) ahead, before its final piece comes.
    pub fn guess(&mut self, tail: &str) {
        let key = normalize(tail);
        if key.is_empty() || self.guess.as_ref().is_some_and(|g| g.key == key) {
            return;
        }
        if let Some(old) = self.guess.take() {
            abort(&old.job.attempts);
        }
        let text = tail.trim().to_string();
        if let Some(a) = self.attempt(&text, false, Priority::End) {
            let now = Instant::now();
            let job = Job { pieces: 0..0, text, attempts: vec![a], first_sent: now, last_sent: now };
            self.guess = Some(Guess { key, job, answer: None });
        }
    }

    /// The key is released: what waits goes now, twice; requests in flight get one more attempt.
    pub fn release(&mut self, tail: &str) {
        self.released = true;
        self.collect();
        let in_flight = self.jobs.len();
        self.send_waiting();
        self.guess_twice(tail);
        // Requests in flight: one more attempt each, oldest first, while there is room.
        for ji in 0..in_flight {
            let job = &self.jobs[ji];
            if job.attempts.len() >= MAX_ATTEMPTS {
                continue;
            }
            let (tried, text) = (job.attempts.iter().any(|a| a.backup), job.text.clone());
            let Some(a) = self.one_more(tried, &text, Priority::End) else { break };
            let job = &mut self.jobs[ji];
            job.attempts.push(a);
            job.last_sent = Instant::now();
        }
    }

    /// The tail as a guess with two attempts running (unless already answered).
    fn guess_twice(&mut self, tail: &str) {
        let key = normalize(tail);
        if key.is_empty() {
            return;
        }
        if !self.guess.as_ref().is_some_and(|g| g.key == key) {
            self.guess(tail);
        }
        let Some(g) = self.guess.as_ref().filter(|g| g.answer.is_none()) else { return };
        if g.job.attempts.iter().filter(|a| !a.over).count() >= 2 || g.job.attempts.len() >= MAX_ATTEMPTS {
            return;
        }
        let (tried, text) = (g.job.attempts.iter().any(|a| a.backup), g.job.text.clone());
        if let Some(a) = self.one_more(tried, &text, Priority::End) {
            let g = self.guess.as_mut().expect("checked");
            g.job.attempts.push(a);
            g.job.last_sent = Instant::now();
        }
    }

    /// Every piece answered, and the tail too (by a guess with its words).
    fn complete(&self, tail_key: &str) -> bool {
        self.jobs.is_empty()
            && self.assigned.iter().all(|&a| a)
            && (tail_key.is_empty() || self.guess.as_ref().is_some_and(|g| g.key == tail_key && g.answer.is_some()))
    }

    /// Waits for the answers until `deadline`. Cleanup: pieces without an answer go as heard
    /// (without filler sounds), always `Ok`. Translate: every piece must be translated, else
    /// `Err(Timeout)` at the deadline.
    pub async fn finish(mut self, tail: &str, deadline: Instant) -> Result<Ends, ProviderError> {
        let released = Instant::now();
        self.released = true;
        let key = normalize(tail);
        self.guess_twice(tail);
        loop {
            self.poll();
            if self.complete(&key) {
                break;
            }
            let now = Instant::now();
            if now >= deadline {
                if self.kind == Kind::Translate {
                    return Err(ProviderError::Timeout);
                }
                break;
            }
            tokio::time::sleep_until(deadline.min(now + Duration::from_millis(10))).await;
        }
        let (mut text, mut raw_pieces) = self.assemble();
        if !key.is_empty() {
            let tail_text = match self.guess.as_ref().filter(|g| g.key == key).and_then(|g| g.answer.clone()) {
                Some(answer) => answer.trim().to_string(),
                None => {
                    raw_pieces += 1;
                    strip_fillers(tail.trim())
                }
            };
            if !tail_text.is_empty() {
                text = if text.is_empty() { tail_text } else { format!("{text} {tail_text}") };
            }
        }
        let pieces = self.segments.len() + usize::from(!key.is_empty());
        tracing::info!(ready_ms = released.elapsed().as_millis() as u64, pieces, raw_pieces, "timing: live end");
        Ok(Ends { text, pieces, raw_pieces, requests: self.requests })
    }

    /// Requests started so far (each attempt counts in usage).
    pub fn requests(&self) -> u32 {
        self.requests
    }

    /// The answers and verbatim pieces in order; unanswered pieces go as heard. Returns the
    /// text and how many pieces had no answer.
    fn assemble(&self) -> (String, usize) {
        let mut out: Vec<String> = Vec::new();
        let mut raw = 0;
        let mut i = 0;
        while i < self.segments.len() {
            if let Some((end, text)) = self.done.get(&i) {
                out.push(text.trim().to_string());
                i = *end;
                continue;
            }
            out.push(strip_fillers(self.segments[i].trim()));
            raw += 1;
            i += 1;
        }
        (out.into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" "), raw)
    }
}

impl Drop for PieceProcessor {
    fn drop(&mut self) {
        for j in &self.jobs {
            abort(&j.attempts);
        }
        if let Some(g) = &self.guess {
            abort(&g.job.attempts);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::routes::RouteHealth;
    use crate::providers::budget::RequestBudget;
    use crate::providers::Completion;
    use std::collections::VecDeque;

    /// Replies `{tag}EN[<transcript>]` after the next delay in the queue (or 100 ms).
    #[derive(Default)]
    struct Fake {
        delays: Mutex<VecDeque<Duration>>,
        calls: Mutex<Vec<(Instant, String)>>,
        finished: Mutex<u32>,
        tag: &'static str,
        /// The first this many requests fail with a 503.
        fail_first: Mutex<u32>,
        rate_limited: bool,
        /// Fixed reply instead of `EN[…]`.
        reply: Option<&'static str>,
    }
    impl Fake {
        fn with(d: &[u64]) -> Self {
            let f = Self::default();
            *f.delays.lock().unwrap() = d.iter().map(|&ms| Duration::from_millis(ms)).collect();
            f
        }
        fn new(d: &[u64]) -> Arc<Self> {
            Arc::new(Self::with(d))
        }
        fn tagged(tag: &'static str, d: &[u64]) -> Arc<Self> {
            Arc::new(Self { tag, ..Self::with(d) })
        }
        fn inputs(&self) -> Vec<String> {
            self.calls.lock().unwrap().iter().map(|(_, t)| t.clone()).collect()
        }
        fn times(&self, t0: Instant) -> Vec<u64> {
            self.calls.lock().unwrap().iter().map(|(t, _)| (*t - t0).as_millis() as u64).collect()
        }
        fn finished(&self) -> u32 {
            *self.finished.lock().unwrap()
        }
    }
    #[async_trait::async_trait]
    impl TextModel for Fake {
        async fn complete(&self, _: &str, user: &str) -> Result<Completion, ProviderError> {
            let text = user.trim_start_matches("<transcript>\n").trim_end_matches("\n</transcript>").to_string();
            self.calls.lock().unwrap().push((Instant::now(), text.clone()));
            let delay = self.delays.lock().unwrap().pop_front().unwrap_or(Duration::from_millis(100));
            tokio::time::sleep(delay).await;
            *self.finished.lock().unwrap() += 1;
            if self.rate_limited {
                return Err(ProviderError::RateLimited { retry_after: None });
            }
            {
                let mut f = self.fail_first.lock().unwrap();
                if *f > 0 {
                    *f -= 1;
                    return Err(ProviderError::Server(503));
                }
            }
            let out = self.reply.map(str::to_string).unwrap_or_else(|| format!("{}EN[{text}]", self.tag));
            Ok(Completion { text: out, total_tokens: 1 })
        }
    }

    struct Rig {
        p: PieceProcessor,
        budget: SharedBudget,
        health: Arc<Mutex<RouteHealth>>,
    }

    fn rig(main: &Arc<Fake>, backup: Option<&Arc<Fake>>) -> Rig {
        rig_kind(main, backup, Kind::Cleanup)
    }

    fn rig_kind(main: &Arc<Fake>, backup: Option<&Arc<Fake>>, kind: Kind) -> Rig {
        let budget: SharedBudget = Arc::new(Mutex::new(RequestBudget::new(14, 3)));
        let health = Arc::new(Mutex::new(RouteHealth::new(3)));
        let models = Models {
            main: main.clone(),
            main_id: "main".into(),
            backup: backup.map(|b| b.clone() as Arc<dyn TextModel>),
            backup_id: "backup".into(),
        };
        let p = PieceProcessor::new(models, "sys".into(), kind, budget.clone(), health.clone());
        Rig { p, budget, health }
    }

    /// Polls every 10 ms of (paused) time for `ms`.
    async fn drive(p: &mut PieceProcessor, ms: u64) {
        for _ in 0..ms / 10 {
            p.poll();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        p.poll();
    }

    /// Times match within one 10 ms poll step.
    fn near(got: Vec<u64>, want: &[u64]) {
        assert_eq!(got.len(), want.len(), "{got:?} vs {want:?}");
        for (g, w) in got.iter().zip(want) {
            assert!(*g >= *w && *g <= w + 10, "{got:?} vs {want:?}");
        }
    }

    fn answer(p: &PieceProcessor, piece: usize) -> Option<String> {
        p.done.get(&piece).map(|(_, t)| t.clone())
    }

    #[tokio::test(start_paused = true)]
    async fn each_piece_goes_at_once() {
        let main = Fake::new(&[]);
        let mut r = rig(&main, None);
        let t0 = Instant::now();
        r.p.push("раз".into());
        drive(&mut r.p, 1_000).await;
        r.p.push("два".into());
        drive(&mut r.p, 50).await;
        assert_eq!(main.inputs(), vec!["раз", "два"]);
        near(main.times(t0), &[0, 1_000]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_request_does_not_hold_the_next() {
        let main = Fake::new(&[5_000, 100]);
        let mut r = rig(&main, None);
        r.p.push("раз".into());
        r.p.push("два".into());
        drive(&mut r.p, 300).await;
        assert_eq!(answer(&r.p, 1).as_deref(), Some("EN[два]"));
        assert_eq!(answer(&r.p, 0), None);
    }

    #[tokio::test(start_paused = true)]
    async fn full_budget_joins_pieces() {
        let main = Fake::new(&[]);
        let mut r = rig(&main, None);
        let t0 = Instant::now();
        for _ in 0..11 {
            assert!(r.budget.lock().unwrap().try_take("main", Priority::Speech, t0));
        }
        for p in ["раз", "два", "три"] {
            r.p.push(p.into());
        }
        drive(&mut r.p, 1_000).await;
        assert!(main.inputs().is_empty());
        drive(&mut r.p, 59_500).await;
        assert_eq!(main.inputs(), vec!["раз два три"]);
        assert_eq!(answer(&r.p, 0).as_deref(), Some("EN[раз два три]"));
    }

    #[tokio::test(start_paused = true)]
    async fn unanswered_piece_is_sent_again_after_2_5_s() {
        let main = Fake::new(&[10_000]);
        let backup = Fake::tagged("B:", &[100]);
        let mut r = rig(&main, Some(&backup));
        let t0 = Instant::now();
        r.p.push("раз".into());
        drive(&mut r.p, 2_700).await;
        near(backup.times(t0), &[2_500]);
        assert_eq!(answer(&r.p, 0).as_deref(), Some("B:EN[раз]"));
        assert_eq!(r.p.requests(), 2);
        drive(&mut r.p, 10_000).await;
        assert_eq!(main.finished(), 0, "the slow attempt is cancelled");
    }

    #[tokio::test(start_paused = true)]
    async fn sick_backup_is_skipped() {
        let main = Fake::new(&[10_000, 100]);
        let backup = Fake::new(&[]);
        let mut r = rig(&main, Some(&backup));
        r.health.lock().unwrap().punish(BACKUP_LITE_ROUTE, Instant::now().into_std());
        r.p.push("раз".into());
        drive(&mut r.p, 2_700).await;
        assert_eq!(main.inputs().len(), 2);
        assert!(backup.inputs().is_empty());
        assert_eq!(answer(&r.p, 0).as_deref(), Some("EN[раз]"));
    }

    #[tokio::test(start_paused = true)]
    async fn slow_backup_gets_punished() {
        let main = Fake::new(&[10_000]);
        let backup = Fake::new(&[10_000]);
        let mut r = rig(&main, Some(&backup));
        r.p.push("раз".into());
        drive(&mut r.p, 5_100).await;
        assert!(!r.health.lock().unwrap().healthy(BACKUP_LITE_ROUTE, Instant::now().into_std()));
    }

    #[tokio::test(start_paused = true)]
    async fn failure_retries_at_once() {
        let main = Arc::new(Fake { fail_first: Mutex::new(1), ..Fake::with(&[200, 100]) });
        let mut r = rig(&main, None);
        let t0 = Instant::now();
        r.p.push("раз".into());
        drive(&mut r.p, 400).await;
        near(main.times(t0), &[0, 200]);
        assert_eq!(answer(&r.p, 0).as_deref(), Some("EN[раз]"));
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limit_closes_the_window() {
        let main = Arc::new(Fake { rate_limited: true, ..Fake::with(&[100]) });
        let mut r = rig(&main, None);
        r.p.push("раз".into());
        drive(&mut r.p, 200).await;
        assert!(!r.budget.lock().unwrap().try_take("main", Priority::End, Instant::now()));
    }

    #[tokio::test(start_paused = true)]
    async fn empty_answer_counts_as_answer() {
        let main = Arc::new(Fake { reply: Some(""), ..Fake::with(&[100]) });
        let mut r = rig(&main, None);
        r.p.push("э-э".into());
        drive(&mut r.p, 3_000).await;
        assert_eq!(answer(&r.p, 0).as_deref(), Some(""));
        assert_eq!(main.inputs().len(), 1);
        assert_eq!(r.p.assemble(), (String::new(), 0));
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_processor_aborts_requests() {
        let main = Fake::new(&[1_000]);
        let mut r = rig(&main, None);
        r.p.push("раз".into());
        drive(&mut r.p, 100).await;
        drop(r.p);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(main.finished(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn guess_hit_needs_no_new_request() {
        let main = Fake::new(&[]);
        let mut r = rig(&main, None);
        r.p.guess("давай завтра");
        drive(&mut r.p, 50).await;
        r.p.push("Давай завтра.".into());
        drive(&mut r.p, 200).await;
        assert_eq!(r.p.requests(), 1);
        assert_eq!(answer(&r.p, 0).as_deref(), Some("EN[давай завтра]"));
    }

    #[tokio::test(start_paused = true)]
    async fn guess_miss_never_uses_guess_text() {
        let main = Fake::new(&[]);
        let mut r = rig(&main, None);
        r.p.guess("по продам");
        drive(&mut r.p, 10).await;
        r.p.push("по продажам".into());
        drive(&mut r.p, 300).await;
        assert_eq!(main.inputs(), vec!["по продам", "по продажам"]);
        assert_eq!(r.p.assemble(), ("EN[по продажам]".to_string(), 0));
    }

    #[tokio::test(start_paused = true)]
    async fn same_tail_is_guessed_once() {
        let main = Fake::new(&[]);
        let mut r = rig(&main, None);
        r.p.guess("давай завтра");
        r.p.guess("Давай, завтра!");
        drive(&mut r.p, 300).await;
        assert_eq!(main.inputs().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn changed_tail_replaces_the_guess() {
        let main = Fake::new(&[1_000, 100]);
        let mut r = rig(&main, None);
        r.p.guess("давай");
        drive(&mut r.p, 50).await;
        r.p.guess("давай завтра");
        r.p.push("давай завтра".into());
        drive(&mut r.p, 2_000).await;
        assert_eq!(main.inputs(), vec!["давай", "давай завтра"]);
        assert_eq!(main.finished(), 1, "the old guess is cancelled");
        assert_eq!(r.p.requests(), 2);
        assert_eq!(answer(&r.p, 0).as_deref(), Some("EN[давай завтра]"));
    }

    #[tokio::test(start_paused = true)]
    async fn guess_counts_in_end_priority() {
        let main = Fake::new(&[]);
        let mut r = rig(&main, None);
        let now = Instant::now();
        for _ in 0..11 {
            assert!(r.budget.lock().unwrap().try_take("main", Priority::Speech, now));
        }
        r.p.guess("давай");
        assert_eq!(main.inputs().len(), 0, "spawned, not yet run");
        drive(&mut r.p, 50).await;
        assert_eq!(main.inputs(), vec!["давай"]);
    }

    const LIMIT: Duration = Duration::from_millis(950);

    #[tokio::test(start_paused = true)]
    async fn release_sends_waiting_pieces_twice() {
        let main = Fake::new(&[]);
        let backup = Fake::tagged("B:", &[]);
        let mut r = rig(&main, Some(&backup));
        let now = Instant::now();
        for _ in 0..11 {
            assert!(r.budget.lock().unwrap().try_take("main", Priority::Speech, now));
        }
        r.p.push("раз".into());
        r.p.push("два".into());
        assert!(main.inputs().is_empty());
        r.p.release("");
        drive(&mut r.p, 10).await;
        assert_eq!(main.inputs(), vec!["раз два"]);
        assert_eq!(backup.inputs(), vec!["раз два"]);
    }

    #[tokio::test(start_paused = true)]
    async fn release_sends_the_tail_as_a_guess() {
        let main = Fake::new(&[]);
        let mut r = rig(&main, None);
        r.p.release("и в три");
        drive(&mut r.p, 10).await;
        assert_eq!(main.inputs(), vec!["и в три", "и в три"], "two attempts at once");
    }

    #[tokio::test(start_paused = true)]
    async fn cleanup_mixes_done_and_raw_at_the_deadline() {
        let main = Fake::new(&[200, 10_000, 10_000, 10_000]);
        let mut r = rig(&main, None);
        r.p.push("раз".into());
        r.p.push("Два, э-э, три.".into());
        drive(&mut r.p, 300).await;
        r.p.release("");
        let t = Instant::now();
        let ends = r.p.finish("", t + LIMIT).await.unwrap();
        assert_eq!(t.elapsed(), LIMIT);
        assert_eq!(ends.text, "EN[раз] Два, три.");
        assert_eq!((ends.pieces, ends.raw_pieces), (2, 1));
    }

    #[tokio::test(start_paused = true)]
    async fn all_answered_finishes_at_once() {
        let main = Fake::new(&[]);
        let mut r = rig(&main, None);
        r.p.push("раз".into());
        drive(&mut r.p, 300).await;
        r.p.release("");
        let t = Instant::now();
        let ends = r.p.finish("", t + LIMIT).await.unwrap();
        assert!(t.elapsed() < Duration::from_millis(10), "{:?}", t.elapsed());
        assert_eq!(ends, Ends { text: "EN[раз]".into(), pieces: 1, raw_pieces: 0, requests: 1 });
    }

    #[tokio::test(start_paused = true)]
    async fn full_budget_pastes_raw_within_limit() {
        let main = Fake::new(&[]);
        let mut r = rig(&main, None);
        let now = Instant::now();
        for _ in 0..14 {
            assert!(r.budget.lock().unwrap().try_take("main", Priority::End, now));
        }
        r.p.push("Ну, э-э, давай.".into());
        r.p.release("и в три");
        let t = Instant::now();
        let ends = r.p.finish("и в три", t + LIMIT).await.unwrap();
        assert!(main.inputs().is_empty());
        assert_eq!(t.elapsed(), LIMIT);
        assert_eq!(ends, Ends { text: "Ну, давай. и в три".into(), pieces: 2, raw_pieces: 2, requests: 0 });
    }

    #[tokio::test(start_paused = true)]
    async fn tail_answered_by_its_guess() {
        let main = Fake::new(&[]);
        let mut r = rig(&main, None);
        r.p.guess("и в три");
        drive(&mut r.p, 300).await;
        r.p.release("и в три");
        let t = Instant::now();
        let ends = r.p.finish("И в три.", t + LIMIT).await.unwrap();
        assert!(t.elapsed() < Duration::from_millis(10));
        assert_eq!((ends.text.as_str(), ends.raw_pieces), ("EN[и в три]", 0));
    }

    #[tokio::test(start_paused = true)]
    async fn translate_waits_for_every_piece() {
        let main = Fake::new(&[2_000, 2_000, 2_000]);
        let mut r = rig_kind(&main, None, Kind::Translate);
        r.p.push("раз".into());
        r.p.release("");
        let t = Instant::now();
        let ends = r.p.finish("", t + Duration::from_secs(4)).await.unwrap();
        assert!(t.elapsed() >= Duration::from_millis(2_000) && t.elapsed() <= Duration::from_millis(2_010), "{:?}", t.elapsed());
        assert_eq!(ends.text, "EN[раз]");
    }

    #[tokio::test(start_paused = true)]
    async fn translate_ladder_adds_a_fresh_main_after_1_s() {
        let main = Fake::new(&[10_000, 100]);
        let backup = Fake::tagged("B:", &[10_000]);
        let mut r = rig_kind(&main, Some(&backup), Kind::Translate);
        let t0 = Instant::now();
        r.p.release("давай");
        let ends = r.p.finish("давай", t0 + Duration::from_secs(4)).await.unwrap();
        near(main.times(t0), &[0, 1_000]);
        assert_eq!(backup.inputs().len(), 1);
        assert_eq!(ends.text, "EN[давай]");
    }

    #[tokio::test(start_paused = true)]
    async fn translate_gives_up_at_its_deadline() {
        let main = Fake::new(&[10_000, 10_000, 10_000, 10_000]);
        let mut r = rig_kind(&main, None, Kind::Translate);
        r.p.push("раз".into());
        r.p.release("");
        let t = Instant::now();
        let out = r.p.finish("", t + Duration::from_secs(4)).await;
        assert_eq!(out, Err(ProviderError::Timeout));
        assert_eq!(t.elapsed(), Duration::from_secs(4));
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_to_translate_is_empty() {
        let main = Fake::new(&[]);
        let mut r = rig_kind(&main, None, Kind::Translate);
        r.p.release("");
        let t = Instant::now();
        let ends = r.p.finish("", t + Duration::from_secs(4)).await.unwrap();
        assert_eq!(ends, Ends { text: String::new(), pieces: 0, raw_pieces: 0, requests: 0 });
    }

    #[tokio::test(start_paused = true)]
    async fn answers_join_in_piece_order() {
        let main = Fake::new(&[500, 100, 10_000]);
        let mut r = rig(&main, None);
        r.p.push("раз".into());
        r.p.push("два".into());
        r.p.push("три, э-э, всё".into());
        drive(&mut r.p, 700).await;
        assert_eq!(r.p.assemble(), ("EN[раз] EN[два] три, всё".to_string(), 1));
    }
}
