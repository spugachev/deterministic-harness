//! The mutex-serialized seat service (ADR-0002).
//!
//! Wraps the single-threaded, IO-free [`SeatMap`] plus the determinism ports in
//! one [`Mutex`], and takes the lock for the whole of every operation. Concurrent
//! callers therefore observe a serial order of ledger operations, so "no
//! overbooking under races" (REQ-005) reduces to the serial property the core
//! proves. Generic over the ports so production wires the real adapters and
//! DST/tests wire deterministic ones.

use core::domain::seats::{SeatError, SeatMap};
use core::ports::{Clock, IdGen};
use std::sync::Mutex;

/// Fixed TTL for a hold, in seconds. A held seat is freed this long after it is
/// taken unless confirmed first.
pub const HOLD_TTL_SECS: i64 = 120;

/// The outcome of a successful hold: its id and the instant it expires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoldGrant {
    /// The unique hold id (for confirm/release).
    pub id: u64,
    /// The instant (Unix seconds) the hold expires.
    pub expires_at: i64,
}

/// The shared, concurrency-safe seat service.
#[derive(Debug)]
pub struct SeatService<C, G> {
    inner: Mutex<Inner<C, G>>,
}

#[derive(Debug)]
struct Inner<C, G> {
    map: SeatMap,
    clock: C,
    ids: G,
}

impl<C: Clock, G: IdGen> SeatService<C, G> {
    /// A new service for an event of `capacity` seats, wired to `clock`/`ids`.
    pub fn new(capacity: u32, clock: C, ids: G) -> Self {
        Self {
            inner: Mutex::new(Inner {
                map: SeatMap::new(capacity),
                clock,
                ids,
            }),
        }
    }

    /// Place a hold for `seats` seats. Reads the current time from the clock,
    /// allocates a fresh id, and expires the hold `HOLD_TTL_SECS` later.
    ///
    /// # Errors
    /// Propagates [`SeatError`] (`ZeroSeatsRequested` / `InsufficientAvailability`).
    ///
    /// # Panics
    /// If the mutex is poisoned by a panic in another holder (cannot happen: the
    /// core is panic-free).
    pub fn hold(&self, seats: u32) -> Result<HoldGrant, SeatError> {
        let mut g = self.inner.lock().expect("seat-service mutex poisoned");
        let now = g.clock.now_unix();
        let expires_at = now.saturating_add(HOLD_TTL_SECS);
        let id = g.ids.next_id();
        g.map
            .hold(id, seats, now, expires_at)
            .map(|id| HoldGrant { id, expires_at })
    }

    /// Confirm a live hold by id.
    ///
    /// # Errors
    /// [`SeatError::UnknownHold`] if no live hold has that id.
    ///
    /// # Panics
    /// If the mutex is poisoned (cannot happen: the core is panic-free).
    pub fn confirm(&self, id: u64) -> Result<(), SeatError> {
        let mut g = self.inner.lock().expect("seat-service mutex poisoned");
        let now = g.clock.now_unix();
        g.map.confirm(id, now)
    }

    /// Release a hold by id. Idempotent: returns `true` iff a hold was freed.
    ///
    /// # Panics
    /// If the mutex is poisoned (cannot happen: the core is panic-free).
    pub fn release(&self, id: u64) -> bool {
        let mut g = self.inner.lock().expect("seat-service mutex poisoned");
        let now = g.clock.now_unix();
        g.map.release(id, now)
    }

    /// Seats currently available at the present time.
    ///
    /// # Panics
    /// If the mutex is poisoned (cannot happen: the core is panic-free).
    #[must_use]
    pub fn available(&self) -> u32 {
        let g = self.inner.lock().expect("seat-service mutex poisoned");
        let now = g.clock.now_unix();
        g.map.available(now)
    }

    /// Confirmed seat count (for assertions/observability).
    ///
    /// # Panics
    /// If the mutex is poisoned (cannot happen: the core is panic-free).
    #[must_use]
    pub fn confirmed(&self) -> u32 {
        let g = self.inner.lock().expect("seat-service mutex poisoned");
        g.map.confirmed()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "unit tests assert on known-good values"
)]
mod tests {
    use super::*;
    use core::ports::{FixedClock, SeqGen};

    fn svc(capacity: u32) -> SeatService<FixedClock, SeqGen> {
        SeatService::new(capacity, FixedClock(1_000), SeqGen(0))
    }

    #[test]
    fn hold_then_confirm_books_seats() {
        let s = svc(10);
        let g = s.hold(3).unwrap();
        assert_eq!(g.expires_at, 1_000 + HOLD_TTL_SECS);
        assert_eq!(s.available(), 7);
        s.confirm(g.id).unwrap();
        assert_eq!(s.confirmed(), 3);
    }

    #[test]
    fn hold_rejects_when_full() {
        let s = svc(2);
        s.hold(2).unwrap();
        assert_eq!(s.hold(1), Err(SeatError::InsufficientAvailability));
    }

    #[test]
    fn release_is_idempotent() {
        let s = svc(5);
        let g = s.hold(2).unwrap();
        assert!(s.release(g.id));
        assert!(!s.release(g.id));
        assert_eq!(s.available(), 5);
    }
}
