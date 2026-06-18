//! Production adapters for the determinism ports.
//!
//! This is the ONE place real wall-clock time enters the system — the
//! production end of the Clock seam (ADR-0001). The domain/application code is
//! forbidden from calling `SystemTime::now` directly (`clippy.toml`); the
//! `#[allow]` below is the sanctioned escape, carrying its reason, precisely
//! because an adapter implementing the `Clock` port is where real time belongs.

use seats_core::ports::{Clock, IdGen};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A [`Clock`] backed by the system wall clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    #[allow(
        clippy::disallowed_methods,
        reason = "the production Clock adapter is the sanctioned place to read real wall-clock time (ADR-0001)"
    )]
    fn now_unix(&self) -> i64 {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        // Saturate into i64; a value beyond i64::MAX seconds is ~292 billion
        // years away, so this never loses real information.
        i64::try_from(secs).unwrap_or(i64::MAX)
    }
}

/// A process-wide monotonic [`IdGen`] backed by an atomic counter. Hold ids are
/// unique within a run, which is all the domain requires (REQ-001).
#[derive(Debug, Default)]
pub struct AtomicIds(AtomicU64);

impl IdGen for AtomicIds {
    fn next_id(&mut self) -> u64 {
        self.0.fetch_add(1, Ordering::Relaxed).saturating_add(1)
    }
}

impl AtomicIds {
    /// Mint the next id through a shared reference — handy when the generator
    /// lives behind shared state rather than being owned mutably.
    pub fn next_shared(&self) -> u64 {
        self.0.fetch_add(1, Ordering::Relaxed).saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::{AtomicIds, SystemClock};
    use seats_core::ports::Clock;

    #[test]
    fn system_clock_is_positive() {
        // A real clock past the epoch returns a positive second count.
        assert!(SystemClock.now_unix() > 0);
    }

    #[test]
    fn atomic_ids_are_unique_and_increasing() {
        let ids = AtomicIds::default();
        let a = ids.next_shared();
        let b = ids.next_shared();
        assert!(b > a);
        assert_eq!(a, 1);
        assert_eq!(b, 2);
    }
}
