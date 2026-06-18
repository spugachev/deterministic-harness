//! Production adapters for the core's determinism ports.
//!
//! These are the ONLY place real non-determinism enters the service: the system
//! wall-clock and a monotonic id counter. Domain/test code never touches these
//! directly — they wire a [`core::ports::Clock`] / [`core::ports::IdGen`] for
//! production while tests/DST wire the deterministic `FixedClock` / `SeqGen`.

use core::ports::{Clock, IdGen};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A [`Clock`] reading the real system wall-clock as whole Unix seconds.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    #[allow(
        clippy::disallowed_methods,
        reason = "the single sanctioned SystemTime::now call: this IS the Clock adapter"
    )]
    fn now_unix(&self) -> i64 {
        // Before the epoch is impossible on a sane host; treat any such skew as 0.
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
    }
}

/// A monotonic [`IdGen`] backed by an atomic counter — unique ids across threads.
#[derive(Debug, Default)]
pub struct AtomicIdGen {
    next: AtomicU64,
}

impl IdGen for AtomicIdGen {
    fn next_id(&mut self) -> u64 {
        // `fetch_add` is monotonic and unique even under concurrent callers.
        self.next.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "unit tests assert on known-good values")]
mod tests {
    use super::*;

    #[test]
    fn system_clock_is_nonnegative_and_monotonicish() {
        let c = SystemClock;
        let a = c.now_unix();
        let b = c.now_unix();
        assert!(a >= 0);
        assert!(b >= a);
    }

    #[test]
    fn atomic_idgen_counts_up_uniquely() {
        let mut g = AtomicIdGen::default();
        assert_eq!(g.next_id(), 1);
        assert_eq!(g.next_id(), 2);
        assert_eq!(g.next_id(), 3);
    }
}
