//! Results of concurrent jobs are released strictly in recording order.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct OrderedDelivery<T> {
    next: u64,
    pending: BTreeMap<u64, T>,
}

impl<T> OrderedDelivery<T> {
    pub fn new(first_id: u64) -> Self {
        Self { next: first_id, pending: BTreeMap::new() }
    }

    /// Accepts any outcome for `id` and returns everything now deliverable, in order.
    pub fn push(&mut self, id: u64, item: T) -> Vec<(u64, T)> {
        if id < self.next {
            return vec![];
        }
        self.pending.insert(id, item);
        let mut ready = Vec::new();
        while let Some(item) = self.pending.remove(&self.next) {
            ready.push((self.next, item));
            self.next += 1;
        }
        ready
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

/// Monotonic job ids starting at 1.
#[derive(Default)]
pub struct JobIds(AtomicU64);

impl JobIds {
    pub fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_order_items_release_immediately() {
        let mut q = OrderedDelivery::new(1);
        assert_eq!(q.push(1, "a"), vec![(1, "a")]);
        assert_eq!(q.push(2, "b"), vec![(2, "b")]);
    }

    #[test]
    fn out_of_order_waits_for_gap() {
        let mut q = OrderedDelivery::new(1);
        assert_eq!(q.push(2, "b"), vec![]);
        assert_eq!(q.push(1, "a"), vec![(1, "a"), (2, "b")]);
    }

    #[test]
    fn failed_job_releases_order() {
        let mut q: OrderedDelivery<Result<&str, &str>> = OrderedDelivery::new(1);
        assert_eq!(q.push(3, Ok("c")), vec![]);
        assert_eq!(q.push(2, Err("boom")), vec![]);
        assert_eq!(q.push(1, Ok("a")), vec![(1, Ok("a")), (2, Err("boom")), (3, Ok("c"))]);
    }

    #[test]
    fn pending_count_tracks_waiting() {
        let mut q = OrderedDelivery::new(1);
        q.push(3, ());
        q.push(2, ());
        assert_eq!(q.pending_count(), 2);
        q.push(1, ());
        assert_eq!(q.pending_count(), 0);
    }

    #[test]
    fn stale_ids_are_dropped() {
        let mut q = OrderedDelivery::new(1);
        q.push(1, "a");
        assert_eq!(q.push(1, "dup"), vec![]);
        assert_eq!(q.pending_count(), 0);
    }

    #[test]
    fn job_ids_are_sequential_from_one() {
        let ids = JobIds::default();
        assert_eq!((ids.next(), ids.next(), ids.next()), (1, 2, 3));
    }
}
