//! Real (production) implementations of the core's determinism ports.
//!
//! These are the ONLY place the banned ambient sources (`SystemTime::now`)
//! appear — they are the seam between the deterministic core and the real
//! world. Tests and the DST harness wire the core's `FixedClock`/`SeqGen`
//! instead, so the domain never observes wall-clock or ambient state.

use core::ports::{Clock, IdGen};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// The system wall clock, in whole seconds since the Unix epoch.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix(&self) -> i64 {
        // The single sanctioned `SystemTime::now` call — the production Clock
        // adapter. Everything downstream takes time through this port.
        #[allow(
            clippy::disallowed_methods,
            reason = "the production Clock adapter is the one sanctioned call site"
        )]
        let now = SystemTime::now();
        i64::try_from(now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())).unwrap_or(i64::MAX)
    }
}

/// A monotonic, thread-safe id source for holds.
#[derive(Debug, Default)]
pub struct AtomicIdGen {
    next: AtomicU64,
}

impl IdGen for AtomicIdGen {
    fn next_id(&mut self) -> u64 {
        // `fetch_add` is monotonic and unique across threads; +1 so ids start
        // at 1 (0 is reserved as "no hold").
        self.next.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "test-only assertions")]
    use super::*;

    #[test]
    fn system_clock_is_nonnegative() {
        assert!(SystemClock.now_unix() >= 0);
    }

    #[test]
    fn atomic_idgen_is_monotonic_and_starts_at_one() {
        let mut g = AtomicIdGen::default();
        assert_eq!(g.next_id(), 1);
        assert_eq!(g.next_id(), 2);
        assert_eq!(g.next_id(), 3);
    }
}
