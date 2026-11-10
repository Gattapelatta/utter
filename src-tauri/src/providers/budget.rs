//! How many text requests each model may still take this minute (spec 2026-11-10 §5.2).
//! The free tier gives the Lite models 15 requests a minute for every dictation and agent
//! together; requests while speaking leave a reserve for the end of a dictation.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

const WINDOW: Duration = Duration::from_secs(60);

/// Who asks for a request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Priority {
    /// A piece while the user speaks: never takes the end reserve.
    Speech,
    /// The end of a dictation and guesses: may take everything left.
    End,
    /// An agent summary: like `Speech`.
    Agent,
}

pub struct RequestBudget {
    per_minute: u32,
    reserve: u32,
    sent: HashMap<String, VecDeque<Instant>>,
    closed_until: HashMap<String, Instant>,
}

pub type SharedBudget = Arc<Mutex<RequestBudget>>;

impl RequestBudget {
    pub fn new(per_minute: u32, reserve: u32) -> Self {
        Self { per_minute, reserve, sent: HashMap::new(), closed_until: HashMap::new() }
    }

    /// Takes one request of `model` if `p` allows it now.
    pub fn try_take(&mut self, model: &str, p: Priority, now: Instant) -> bool {
        if self.closed_until.get(model).is_some_and(|&t| now < t) {
            return false;
        }
        let sent = self.sent.entry(model.to_string()).or_default();
        while sent.front().is_some_and(|&t| now.saturating_duration_since(t) >= WINDOW) {
            sent.pop_front();
        }
        let limit = match p {
            Priority::End => self.per_minute,
            Priority::Speech | Priority::Agent => self.per_minute.saturating_sub(self.reserve),
        };
        if sent.len() as u32 >= limit {
            return false;
        }
        sent.push_back(now);
        true
    }

    /// The model answered 429: no request until `retry_after` (or a minute).
    pub fn rate_limited(&mut self, model: &str, retry_after: Option<Duration>, now: Instant) {
        self.closed_until.insert(model.to_string(), now + retry_after.unwrap_or(WINDOW));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(b: &mut RequestBudget, model: &str, p: Priority, n: usize, now: Instant) -> usize {
        (0..n).filter(|_| b.try_take(model, p, now)).count()
    }

    #[tokio::test(start_paused = true)]
    async fn speech_keeps_the_end_reserve() {
        let mut b = RequestBudget::new(14, 3);
        let now = Instant::now();
        assert_eq!(take(&mut b, "m", Priority::Speech, 12, now), 11);
        assert_eq!(take(&mut b, "m", Priority::End, 4, now), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn agent_is_like_speech() {
        let mut b = RequestBudget::new(14, 3);
        let now = Instant::now();
        assert_eq!(take(&mut b, "m", Priority::Speech, 11, now), 11);
        assert!(!b.try_take("m", Priority::Agent, now));
        assert!(b.try_take("m", Priority::End, now));
    }

    #[tokio::test(start_paused = true)]
    async fn window_slides_after_a_minute() {
        let mut b = RequestBudget::new(14, 3);
        let now = Instant::now();
        assert_eq!(take(&mut b, "m", Priority::End, 14, now), 14);
        assert!(!b.try_take("m", Priority::End, now + Duration::from_secs(59)));
        assert!(b.try_take("m", Priority::End, now + WINDOW + Duration::from_millis(1)));
    }

    #[tokio::test(start_paused = true)]
    async fn models_have_their_own_windows() {
        let mut b = RequestBudget::new(14, 3);
        let now = Instant::now();
        assert_eq!(take(&mut b, "a", Priority::End, 14, now), 14);
        assert!(b.try_take("b", Priority::Speech, now));
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limited_closes_the_window() {
        let mut b = RequestBudget::new(14, 3);
        let now = Instant::now();
        b.rate_limited("m", Some(Duration::from_secs(20)), now);
        assert!(!b.try_take("m", Priority::End, now + Duration::from_secs(19)));
        assert!(b.try_take("m", Priority::End, now + Duration::from_secs(20)));
        b.rate_limited("n", None, now);
        assert!(!b.try_take("n", Priority::End, now + Duration::from_secs(59)));
        assert!(b.try_take("n", Priority::End, now + WINDOW));
    }
}
