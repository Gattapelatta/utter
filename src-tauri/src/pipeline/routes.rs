//! Backup models (spec 2026-11-04 §4.5): the same audio goes down the next route when
//! the current one is slow or fails; the first answer wins.

use std::future::Future;
use std::pin::Pin;
use std::task::Poll;
use std::time::{Duration, Instant};

use crate::providers::ProviderError;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Route {
    /// One request: audio in, final text out.
    Direct { model: &'static str },
    /// Speech-to-text, then cleanup or translation of the text.
    TwoStep { stt: &'static str, text: &'static str },
}

/// A route that was slow or failed goes to the end of the list for this long.
pub const PUNISH_FOR: Duration = Duration::from_secs(10 * 60);

/// How long a route may stay silent before the next one starts in parallel.
pub fn hedge_after(audio_secs: f32) -> Duration {
    Duration::from_millis(2_500) + Duration::from_secs_f32(0.1 * audio_secs.max(0.0))
}

/// Remembers which routes were recently slow or failing.
#[derive(Clone, Debug)]
pub struct RouteHealth {
    punished_until: Vec<Option<Instant>>,
}

impl RouteHealth {
    pub fn new(routes: usize) -> Self {
        Self { punished_until: vec![None; routes] }
    }

    fn punished(&self, route: usize, now: Instant) -> bool {
        self.punished_until[route].is_some_and(|t| now < t)
    }

    /// Healthy routes in their order, then punished ones in their order.
    pub fn order(&self, now: Instant) -> Vec<usize> {
        let n = self.punished_until.len();
        let healthy = (0..n).filter(|&r| !self.punished(r, now));
        let punished = (0..n).filter(|&r| self.punished(r, now));
        healthy.chain(punished).collect()
    }

    pub fn punish(&mut self, route: usize, now: Instant) {
        if let Some(slot) = self.punished_until.get_mut(route) {
            *slot = Some(now + PUNISH_FOR);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AttemptOutcome {
    Won,
    Failed(ProviderError),
    /// Still running (and not yet overtaken) when another attempt won.
    Cancelled,
    /// Had not answered when the next attempt was started.
    Slow,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AttemptReport {
    /// Position in the list passed to `run_hedged`.
    pub index: usize,
    pub outcome: AttemptOutcome,
}

pub type Attempt<T> = Pin<Box<dyn Future<Output = Result<T, ProviderError>> + Send>>;

/// Starts the first attempt; the next one after `hedge_after` without an answer, or at
/// once when the running one fails. The first success wins and the others are dropped
/// (cancelled). `Unauthorized` returns at once: the key is the same for every route.
/// Reports cover started attempts only.
pub async fn run_hedged<T>(attempts: Vec<Attempt<T>>, hedge_after: Duration) -> (Result<T, ProviderError>, Vec<AttemptReport>) {
    let mut waiting = attempts.into_iter();
    let mut running: Vec<Option<Attempt<T>>> = Vec::new();
    let mut slow: Vec<bool> = Vec::new();
    let mut reports: Vec<AttemptReport> = Vec::new();
    let mut last_err = ProviderError::BadResponse("no routes".into());

    // Overtaken attempts that were still running when the race ended.
    let close = |running: &[Option<Attempt<T>>], slow: &[bool], reports: &mut Vec<AttemptReport>| {
        for (index, slot) in running.iter().enumerate() {
            if slot.is_some() {
                let outcome = if slow[index] { AttemptOutcome::Slow } else { AttemptOutcome::Cancelled };
                reports.push(AttemptReport { index, outcome });
            }
        }
        reports.sort_by_key(|r| r.index);
    };

    let Some(first) = waiting.next() else { return (Err(last_err), reports) };
    running.push(Some(first));
    slow.push(false);
    let mut deadline = tokio::time::Instant::now() + hedge_after;

    enum Step<T> {
        Done(usize, Result<T, ProviderError>),
        Hedge,
    }
    loop {
        let has_waiting = waiting.len() > 0;
        let next_done = std::future::poll_fn(|cx| {
            for (i, slot) in running.iter_mut().enumerate() {
                if let Some(f) = slot {
                    if let Poll::Ready(r) = f.as_mut().poll(cx) {
                        *slot = None;
                        return Poll::Ready(Step::Done(i, r));
                    }
                }
            }
            Poll::Pending
        });
        let step = tokio::select! {
            s = next_done => s,
            _ = tokio::time::sleep_until(deadline), if has_waiting => Step::Hedge,
        };
        let start_next = match step {
            Step::Done(index, Ok(v)) => {
                reports.push(AttemptReport { index, outcome: AttemptOutcome::Won });
                close(&running, &slow, &mut reports);
                return (Ok(v), reports);
            }
            Step::Done(index, Err(e)) => {
                reports.push(AttemptReport { index, outcome: AttemptOutcome::Failed(e.clone()) });
                if e == ProviderError::Unauthorized {
                    close(&running, &slow, &mut reports);
                    return (Err(e), reports);
                }
                last_err = e;
                true
            }
            Step::Hedge => {
                for (i, slot) in running.iter().enumerate() {
                    if slot.is_some() {
                        slow[i] = true;
                    }
                }
                true
            }
        };
        if start_next {
            match waiting.next() {
                Some(f) => {
                    running.push(Some(f));
                    slow.push(false);
                    deadline = tokio::time::Instant::now() + hedge_after;
                }
                None if running.iter().all(Option::is_none) => {
                    reports.sort_by_key(|r| r.index);
                    return (Err(last_err), reports);
                }
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn after(ms: u64, r: Result<u32, ProviderError>) -> Attempt<u32> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            r
        })
    }

    fn outcomes(reports: &[AttemptReport]) -> Vec<(usize, AttemptOutcome)> {
        reports.iter().map(|r| (r.index, r.outcome.clone())).collect()
    }

    const HEDGE: Duration = Duration::from_millis(3_000);

    #[tokio::test(start_paused = true)]
    async fn first_answer_wins() {
        let t = tokio::time::Instant::now();
        let (r, reports) = run_hedged(vec![after(1_000, Ok(0)), after(10, Ok(1))], HEDGE).await;
        assert_eq!(r, Ok(0));
        assert_eq!(outcomes(&reports), vec![(0, AttemptOutcome::Won)]);
        assert_eq!(t.elapsed(), Duration::from_millis(1_000));
    }

    #[tokio::test(start_paused = true)]
    async fn slow_first_starts_second_after_hedge() {
        let t = tokio::time::Instant::now();
        let (r, reports) = run_hedged(vec![after(20_000, Ok(0)), after(1_000, Ok(1))], HEDGE).await;
        assert_eq!(r, Ok(1));
        assert_eq!(outcomes(&reports), vec![(0, AttemptOutcome::Slow), (1, AttemptOutcome::Won)]);
        assert_eq!(t.elapsed(), HEDGE + Duration::from_millis(1_000));
    }

    #[tokio::test(start_paused = true)]
    async fn error_starts_next_at_once() {
        let t = tokio::time::Instant::now();
        let (r, reports) = run_hedged(vec![after(100, Err(ProviderError::Server(503))), after(1_000, Ok(1))], HEDGE).await;
        assert_eq!(r, Ok(1));
        assert_eq!(
            outcomes(&reports),
            vec![(0, AttemptOutcome::Failed(ProviderError::Server(503))), (1, AttemptOutcome::Won)]
        );
        assert_eq!(t.elapsed(), Duration::from_millis(1_100));
    }

    #[tokio::test(start_paused = true)]
    async fn all_fail_returns_last_error() {
        let rate = ProviderError::RateLimited { retry_after: None };
        let (r, reports) = run_hedged(
            vec![after(100, Err(ProviderError::Server(500))), after(100, Err(ProviderError::Timeout)), after(100, Err(rate.clone()))],
            HEDGE,
        )
        .await;
        assert_eq!(r, Err(rate));
        assert_eq!(reports.len(), 3);
        assert!(reports.iter().all(|r| matches!(r.outcome, AttemptOutcome::Failed(_))));
    }

    #[tokio::test(start_paused = true)]
    async fn unauthorized_stops_immediately() {
        let (r, reports) = run_hedged(vec![after(100, Err(ProviderError::Unauthorized)), after(10, Ok(1))], HEDGE).await;
        assert_eq!(r, Err(ProviderError::Unauthorized));
        assert_eq!(outcomes(&reports), vec![(0, AttemptOutcome::Failed(ProviderError::Unauthorized))]);
    }

    #[tokio::test(start_paused = true)]
    async fn slow_attempt_can_still_win_after_hedge() {
        let (r, reports) = run_hedged(vec![after(3_500, Ok(0)), after(2_000, Ok(1))], HEDGE).await;
        assert_eq!(r, Ok(0));
        assert_eq!(outcomes(&reports), vec![(0, AttemptOutcome::Won), (1, AttemptOutcome::Cancelled)]);
    }

    #[tokio::test]
    async fn no_attempts_is_an_error() {
        let (r, reports) = run_hedged::<u32>(vec![], HEDGE).await;
        assert!(r.is_err());
        assert!(reports.is_empty());
    }

    #[test]
    fn hedge_grows_with_audio() {
        assert_eq!(hedge_after(0.0), Duration::from_millis(2_500));
        assert_eq!(hedge_after(10.0), Duration::from_millis(3_500));
    }

    #[test]
    fn health_moves_punished_to_end_for_10_min() {
        let t0 = Instant::now();
        let mut h = RouteHealth::new(3);
        assert_eq!(h.order(t0), vec![0, 1, 2]);
        h.punish(0, t0);
        assert_eq!(h.order(t0 + Duration::from_secs(1)), vec![1, 2, 0]);
        h.punish(1, t0);
        assert_eq!(h.order(t0 + Duration::from_secs(599)), vec![2, 0, 1]);
        assert_eq!(h.order(t0 + Duration::from_secs(601)), vec![0, 1, 2]);
    }
}
