//! Permission requests waiting for the user's click. Each waits at most `PERMISSION_WAIT` from
//! the moment its card is on screen; until then (a dictation is on, another card is first) it waits.

pub use crate::agents::wire::Reply;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

pub const PERMISSION_WAIT: Duration = Duration::from_secs(30);

struct Pending {
    tx: oneshot::Sender<Reply>,
    shown_at: Option<Instant>,
}

#[derive(Default)]
pub struct Broker {
    pending: Mutex<HashMap<u64, Pending>>,
}

impl Broker {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Pending>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn open(&self, id: u64) -> oneshot::Receiver<Reply> {
        let (tx, rx) = oneshot::channel();
        self.lock().insert(id, Pending { tx, shown_at: None });
        rx
    }

    /// Hands the answer to the waiting helper. `false` if nobody waits for it any more.
    pub fn answer(&self, id: u64, r: Reply) -> bool {
        let Some(p) = self.lock().remove(&id) else { return false };
        p.tx.send(r).is_ok()
    }

    /// The card is on screen: the wait counts from the first time it was.
    pub fn shown(&self, id: u64, now: Instant) {
        if let Some(p) = self.lock().get_mut(&id) {
            p.shown_at.get_or_insert(now);
        }
    }

    /// Requests on screen for `PERMISSION_WAIT`, and those whose helper is gone, end with no
    /// decision. Returns their ids.
    pub fn expire(&self, now: Instant) -> Vec<u64> {
        let mut pending = self.lock();
        let gone: Vec<u64> = pending
            .iter()
            .filter(|(_, p)| p.tx.is_closed() || p.shown_at.is_some_and(|at| now.duration_since(at) >= PERMISSION_WAIT))
            .map(|(id, _)| *id)
            .collect();
        for id in &gone {
            if let Some(p) = pending.remove(id) {
                let _ = p.tx.send(Reply::NoDecision);
            }
        }
        gone
    }

    /// Ids still waiting.
    pub fn pending(&self) -> Vec<u64> {
        let mut pending = self.lock();
        pending.retain(|_, p| !p.tx.is_closed());
        pending.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn broker_answer_reaches_waiter() {
        let b = Broker::default();
        let rx = b.open(1);
        assert!(b.answer(1, Reply::Allow { always: true }));
        assert_eq!(rx.await.unwrap(), Reply::Allow { always: true });
        // Answered once: a second answer goes nowhere.
        assert!(!b.answer(1, Reply::Deny));
    }

    #[tokio::test]
    async fn broker_expires_30s_after_shown_not_before() {
        let t = Instant::now();
        let b = Broker::default();
        let rx = b.open(7);
        // Never shown: never expires, however long it waits.
        assert!(b.expire(t + Duration::from_secs(600)).is_empty());
        b.shown(7, t + Duration::from_secs(100));
        // Shown again later (the card came back after a dictation): the first time counts.
        b.shown(7, t + Duration::from_secs(120));
        assert!(b.expire(t + Duration::from_secs(129)).is_empty());
        assert_eq!(b.expire(t + Duration::from_secs(130)), vec![7]);
        assert_eq!(rx.await.unwrap(), Reply::NoDecision);
        assert!(b.expire(t + Duration::from_secs(999)).is_empty());
    }

    #[tokio::test]
    async fn two_requests_independent() {
        let t = Instant::now();
        let b = Broker::default();
        let rx1 = b.open(1);
        let rx2 = b.open(2);
        b.shown(1, t);
        assert_eq!(b.expire(t + PERMISSION_WAIT), vec![1]);
        assert_eq!(rx1.await.unwrap(), Reply::NoDecision);
        b.answer(2, Reply::Deny);
        assert_eq!(rx2.await.unwrap(), Reply::Deny);
    }

    #[tokio::test]
    async fn gone_waiter_is_forgotten() {
        // utter-agent gave up (Claude Code's own timeout): answering it must not fail loudly.
        let b = Broker::default();
        drop(b.open(3));
        assert!(!b.answer(3, Reply::Deny));
        assert!(b.pending().is_empty());
    }
}
