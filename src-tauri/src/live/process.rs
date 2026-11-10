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
use crate::pipeline::{postprocess, user_message};
use crate::providers::budget::{Priority, SharedBudget};
use crate::providers::defaults::BACKUP_LITE_ROUTE;
use crate::providers::{ProviderError, TextModel};

/// A request without an answer this long while speaking is sent once more (the backup Lite
/// model while healthy); a backup silent this long is taken as sick.
pub const RETRY_AFTER: Duration = Duration::from_millis(2_500);
/// Attempts one request may have in all; past them it waits for the ones still running.
const MAX_ATTEMPTS: usize = 4;

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

pub struct PieceProcessor {
    models: Models,
    system: String,
    #[allow(dead_code)] // read by `finish` (plan Task 5)
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
            requests: 0,
        }
    }

    /// A final piece from the recogniser.
    pub fn push(&mut self, segment: String) {
        self.segments.push(segment);
        self.assigned.push(false);
        self.poll();
    }

    fn priority(&self) -> Priority {
        Priority::Speech
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
            for a in job.attempts.iter_mut().filter(|a| !a.over) {
                match a.reply.try_recv() {
                    Ok(Ok(text)) => {
                        answered.push((ji, text));
                        break;
                    }
                    Ok(Err(e)) => {
                        a.over = true;
                        errors.push((a.backup, e));
                    }
                    Err(oneshot::error::TryRecvError::Closed) => a.over = true,
                    Err(oneshot::error::TryRecvError::Empty) => {
                        if a.backup && !a.judged && now.saturating_duration_since(a.sent) >= RETRY_AFTER {
                            a.judged = true;
                            slow_backup = true;
                        }
                    }
                }
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
            for a in &job.attempts {
                a.task.abort();
            }
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
        let p = self.priority();
        for ji in 0..self.jobs.len() {
            let job = &self.jobs[ji];
            let running = job.attempts.iter().filter(|a| !a.over).count();
            let due = running == 0 || now.saturating_duration_since(job.last_sent) >= RETRY_AFTER;
            if !due || job.attempts.len() >= MAX_ATTEMPTS {
                continue;
            }
            let try_backup = !job.attempts.iter().any(|a| a.backup) && self.backup_healthy();
            let text = job.text.clone();
            let started = self.attempt(&text, try_backup, p).or_else(|| if try_backup { self.attempt(&text, false, p) } else { None });
            if let Some(a) = started {
                let job = &mut self.jobs[ji];
                tracing::info!(attempt = job.attempts.len() + 1, waited_ms = now.saturating_duration_since(job.first_sent).as_millis() as u64, "timing: live text sent again");
                job.attempts.push(a);
                job.last_sent = now;
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
            self.assigned[pieces.clone()].iter_mut().for_each(|x| *x = true);
            let now = Instant::now();
            self.jobs.push(Job { pieces, text, attempts: vec![a], first_sent: now, last_sent: now });
        }
    }

    fn joined(&self, pieces: &Range<usize>) -> String {
        self.segments[pieces.clone()].iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ")
    }

    /// Requests started so far (each attempt counts in usage).
    pub fn requests(&self) -> u32 {
        self.requests
    }

    /// The answers and verbatim pieces in order; unanswered pieces go as heard. Returns the
    /// text and how many pieces had no answer.
    #[cfg_attr(not(test), allow(dead_code))] // used by `finish` (plan Task 5)
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
            for a in &j.attempts {
                a.task.abort();
            }
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
        let budget: SharedBudget = Arc::new(Mutex::new(RequestBudget::new(14, 3)));
        let health = Arc::new(Mutex::new(RouteHealth::new(3)));
        let models = Models {
            main: main.clone(),
            main_id: "main".into(),
            backup: backup.map(|b| b.clone() as Arc<dyn TextModel>),
            backup_id: "backup".into(),
        };
        let p = PieceProcessor::new(models, "sys".into(), Kind::Cleanup, budget.clone(), health.clone());
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
