//! The serialized application state (REQ-007, ADR-0002).
//!
//! The verified core ([`SeatMap`]) is single-threaded and pure. This wrapper
//! gives the concurrent service its safety by holding the map behind ONE mutex
//! and minting hold ids from one id source under the same lock: every
//! operation is fully serialized, so "no overbooking under concurrency"
//! reduces to "no overbooking over any serial sequence" — exactly what
//! proptest and Kani prove on the core.
//!
//! Generic over the [`Clock`]/[`IdGen`] ports so production wires the real
//! adapters and the DST harness wires deterministic seeded ones.

use core::domain::seats::{HoldError, SeatMap};
use core::ports::{Clock, IdGen};
use std::sync::Mutex;

/// Default hold TTL in seconds (two minutes).
pub const DEFAULT_TTL_SECS: i64 = 120;

/// Outcome of a hold request exposed to the transport layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Granted {
    /// The minted hold id.
    pub id: u64,
    /// Seats reserved.
    pub seats: u32,
    /// Expiry instant (whole seconds since the Unix epoch).
    pub expires_at: i64,
}

/// The shared, serialized service state.
#[derive(Debug)]
pub struct AppState<C, G> {
    inner: Mutex<Inner<G>>,
    clock: C,
    ttl_secs: i64,
}

#[derive(Debug)]
struct Inner<G> {
    map: SeatMap,
    ids: G,
}

impl<C: Clock, G: IdGen> AppState<C, G> {
    /// Build state for an event of `capacity` seats with the given ports and a
    /// hold TTL.
    pub const fn new(capacity: u32, clock: C, ids: G, ttl_secs: i64) -> Self {
        Self {
            inner: Mutex::new(Inner {
                map: SeatMap::new(capacity),
                ids,
            }),
            clock,
            ttl_secs,
        }
    }

    /// Request a hold of `seats` seats (REQ-001). The id is minted and the
    /// ledger mutated under one lock, so concurrent requests cannot both win
    /// the last seats.
    ///
    /// # Errors
    /// Propagates [`HoldError`] from the core (insufficient availability, zero
    /// seats).
    ///
    /// # Panics
    /// If the state mutex is poisoned by a panic in another thread.
    pub fn hold(&self, seats: u32) -> Result<Granted, HoldError> {
        let now = self.clock.now_unix();
        let mut g = self.inner.lock().expect("state mutex poisoned");
        let id = g.ids.next_id();
        let h = g.map.hold(id, seats, now, self.ttl_secs)?;
        Ok(Granted {
            id: h.id,
            seats: h.seats,
            expires_at: h.expires_at,
        })
    }

    /// Confirm a live hold (REQ-003).
    ///
    /// # Errors
    /// [`HoldError::NotHeld`] if the hold is unknown, expired, or already
    /// confirmed/released.
    ///
    /// # Panics
    /// If the state mutex is poisoned.
    pub fn confirm(&self, id: u64) -> Result<u32, HoldError> {
        let now = self.clock.now_unix();
        let mut g = self.inner.lock().expect("state mutex poisoned");
        g.map.confirm(id, now)
    }

    /// Release a hold (REQ-004). Idempotent no-op for unknown/expired ids.
    ///
    /// # Panics
    /// If the state mutex is poisoned.
    pub fn release(&self, id: u64) {
        let now = self.clock.now_unix();
        let mut g = self.inner.lock().expect("state mutex poisoned");
        g.map.release(id, now);
    }

    /// Seats currently available (REQ-006).
    ///
    /// # Panics
    /// If the state mutex is poisoned.
    pub fn available(&self) -> u32 {
        let now = self.clock.now_unix();
        let g = self.inner.lock().expect("state mutex poisoned");
        g.map.available(now)
    }

    /// Seats occupied right now: confirmed plus still-live holds. Exposed for
    /// the DST harness's no-overbooking assertion (REQ-007) — `available`
    /// saturates at zero and so cannot witness an overbooking.
    ///
    /// # Panics
    /// If the state mutex is poisoned.
    pub fn occupied(&self) -> u32 {
        let now = self.clock.now_unix();
        let g = self.inner.lock().expect("state mutex poisoned");
        g.map.occupied(now)
    }

    /// The venue capacity.
    ///
    /// # Panics
    /// If the state mutex is poisoned.
    pub fn capacity(&self) -> u32 {
        let g = self.inner.lock().expect("state mutex poisoned");
        g.map.capacity()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "test-only assertions")]
    use super::*;
    use core::ports::{FixedClock, SeqGen};

    fn state(capacity: u32) -> AppState<FixedClock, SeqGen> {
        AppState::new(capacity, FixedClock(1000), SeqGen(0), DEFAULT_TTL_SECS)
    }

    #[test]
    fn hold_mints_unique_ascending_ids() {
        let s = state(10);
        let a = s.hold(1).unwrap();
        let b = s.hold(1).unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(a.expires_at, 1000 + DEFAULT_TTL_SECS);
    }

    #[test]
    fn hold_confirm_release_available_flow() {
        let s = state(5);
        let g = s.hold(2).unwrap();
        assert_eq!(s.available(), 3);
        assert_eq!(s.confirm(g.id).unwrap(), 2);
        assert_eq!(s.available(), 3);
        let g2 = s.hold(1).unwrap();
        s.release(g2.id);
        assert_eq!(s.available(), 3);
        assert_eq!(s.capacity(), 5);
    }

    #[test]
    fn hold_rejected_when_full() {
        let s = state(1);
        s.hold(1).unwrap();
        assert_eq!(s.hold(1), Err(HoldError::InsufficientAvailability));
    }
}
