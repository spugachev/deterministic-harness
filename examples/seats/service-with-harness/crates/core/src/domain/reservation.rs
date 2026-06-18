//! The seat-reservation domain — pure, IO-free, panic-free.
//!
//! A [`Reservation`] tracks a single event with a fixed `capacity`. Seats are
//! either confirmed (permanently booked) or currently held (reserved with a TTL
//! that expires lazily based on the current time). All non-determinism — the
//! clock and id generation — flows through the [`Clock`]/[`IdGen`] ports, so the
//! whole domain is reproducible and a natural Kani/proptest target.
//!
//! The capacity invariant (REQ-005) is the heart of the type: at every point,
//! `confirmed + active_held <= capacity`. It holds by construction because the
//! only function that grows the reserved total — [`Reservation::hold`] — first
//! reclaims expired holds and then refuses unless enough seats remain.

use crate::domain::hold::{next, HoldEvent, HoldState};
use crate::ports::{Clock, IdGen};

/// A granted hold: its id, how many seats it reserves, and the unix second at
/// which it expires (exclusive — it is still valid AT `expires_at`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hold {
    /// Unique id assigned by the [`IdGen`] port.
    pub id: u64,
    /// Number of seats this hold reserves.
    pub seats: u32,
    /// Unix second at which the hold expires; valid while `now <= expires_at`.
    pub expires_at: i64,
}

impl Hold {
    /// Whether this hold is still live at `now` (not yet expired).
    #[must_use]
    pub fn is_active(&self, now: i64) -> bool {
        now <= self.expires_at
    }
}

/// Why a [`Reservation::hold`] request was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldError {
    /// Fewer than the requested number of seats are available.
    InsufficientAvailability,
    /// A hold for zero seats is meaningless and always rejected.
    ZeroSeats,
}

/// Why a [`Reservation::confirm`] request was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmError {
    /// No live hold with that id (unknown, already confirmed, released, or
    /// expired). The FSM models all of these as "no `Confirm` transition".
    NotConfirmable,
}

/// The reservation aggregate for one event. Holds are stored in a flat vector;
/// the domain is small and the harness gates favour obvious code over cleverness.
#[derive(Clone, Debug)]
pub struct Reservation {
    capacity: u32,
    ttl_secs: i64,
    confirmed: u32,
    holds: Vec<Hold>,
}

impl Reservation {
    /// Create an empty reservation for an event with `capacity` seats whose
    /// holds live for `ttl_secs` seconds. A non-positive TTL is clamped to 0
    /// (every hold expires immediately) so the type is total.
    #[must_use]
    pub fn new(capacity: u32, ttl_secs: i64) -> Self {
        Self {
            capacity,
            ttl_secs: ttl_secs.max(0),
            confirmed: 0,
            holds: Vec::new(),
        }
    }

    /// Like [`Reservation::new`] but pre-allocates room for `hold_hint` live
    /// holds. Behaviourally identical to `new` (capacity is just a hint); it
    /// exists so the Kani proof can avoid reasoning about `Vec` reallocation,
    /// which otherwise blows up CBMC. Production code uses [`Reservation::new`].
    #[must_use]
    pub fn with_hold_capacity(capacity: u32, ttl_secs: i64, hold_hint: usize) -> Self {
        Self {
            capacity,
            ttl_secs: ttl_secs.max(0),
            confirmed: 0,
            holds: Vec::with_capacity(hold_hint),
        }
    }

    /// Total seats permanently booked.
    #[must_use]
    pub fn confirmed(&self) -> u32 {
        self.confirmed
    }

    /// Seats held by live (un-expired) holds at `now`.
    #[must_use]
    pub fn held(&self, now: i64) -> u32 {
        self.holds
            .iter()
            .filter(|h| h.is_active(now))
            .fold(0_u32, |acc, h| acc.saturating_add(h.seats))
    }

    /// Seats currently available to be held or confirmed at `now`:
    /// `capacity - confirmed - active_held` (never underflows).
    #[must_use]
    pub fn available(&self, now: i64) -> u32 {
        self.capacity
            .saturating_sub(self.confirmed)
            .saturating_sub(self.held(now))
    }

    /// Drop holds that have expired at `now`. Lazy expiry (REQ-004): expired
    /// holds free their seats the moment any operation observes the clock.
    fn reclaim_expired(&mut self, now: i64) {
        self.holds.retain(|h| h.is_active(now));
    }

    /// Request a hold for `seats` seats (REQ-001). Reclaims expired holds first,
    /// then grants iff at least `seats` are available. The new hold expires at
    /// `now + ttl`. Returns the granted [`Hold`] or a [`HoldError`].
    ///
    /// # Errors
    /// [`HoldError::ZeroSeats`] for a zero request; [`HoldError::InsufficientAvailability`]
    /// when fewer than `seats` seats remain.
    pub fn hold<C: Clock, I: IdGen>(
        &mut self,
        seats: u32,
        clock: &C,
        ids: &mut I,
    ) -> Result<Hold, HoldError> {
        if seats == 0 {
            return Err(HoldError::ZeroSeats);
        }
        let now = clock.now_unix();
        self.reclaim_expired(now);
        if self.available(now) < seats {
            return Err(HoldError::InsufficientAvailability);
        }
        let hold = Hold {
            id: ids.next_id(),
            seats,
            expires_at: now.saturating_add(self.ttl_secs),
        };
        self.holds.push(hold);
        Ok(hold)
    }

    /// Find the index of a live hold with `id` at `now`, if any.
    fn active_index(&self, id: u64, now: i64) -> Option<usize> {
        self.holds
            .iter()
            .position(|h| h.id == id && h.is_active(now))
    }

    /// Confirm a live hold by id (REQ-002): its seats become permanently booked
    /// and the hold is removed from the live set. Confirming an unknown,
    /// expired, released, or already-confirmed hold is rejected — modelled as
    /// the FSM's missing `Confirm` transition out of a terminal state.
    ///
    /// # Errors
    /// [`ConfirmError::NotConfirmable`] when no live hold with `id` exists.
    pub fn confirm<C: Clock>(&mut self, id: u64, clock: &C) -> Result<(), ConfirmError> {
        let now = clock.now_unix();
        self.reclaim_expired(now);
        let idx = self
            .active_index(id, now)
            .ok_or(ConfirmError::NotConfirmable)?;
        // FSM gate: only a Held hold confirms. Live holds are always Held here,
        // but routing through `next` keeps the lifecycle the single source of
        // truth (REQ-002).
        if next(HoldState::Held, HoldEvent::Confirm) != Some(HoldState::Confirmed) {
            return Err(ConfirmError::NotConfirmable);
        }
        let hold = self.holds.swap_remove(idx);
        self.confirmed = self.confirmed.saturating_add(hold.seats);
        Ok(())
    }

    /// Release an unconfirmed hold by id (REQ-003). Idempotent: releasing an
    /// unknown or expired hold is a no-op that still reports success, so a
    /// retried release never errors. Returns whether a live hold was removed.
    pub fn release<C: Clock>(&mut self, id: u64, clock: &C) -> bool {
        let now = clock.now_unix();
        self.reclaim_expired(now);
        match self.active_index(id, now) {
            Some(idx) => {
                self.holds.swap_remove(idx);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ConfirmError, HoldError, Reservation};
    use crate::ports::{FixedClock, SeqGen};

    const TTL: i64 = 60;

    #[test]
    fn hold_then_available_decreases() {
        let mut r = Reservation::new(10, TTL);
        let clock = FixedClock(1_000);
        let mut ids = SeqGen(0);
        let h = r.hold(3, &clock, &mut ids).expect("3 of 10 available");
        assert_eq!(h.seats, 3);
        assert_eq!(h.expires_at, 1_060);
        assert_eq!(r.available(1_000), 7);
        assert_eq!(r.held(1_000), 3);
    }

    #[test]
    fn with_hold_capacity_behaves_like_new() {
        // The pre-sized constructor (used only by the Kani proof) is
        // behaviourally identical to `new`: same capacity, TTL clamping, and
        // hold accounting. The pre-allocation is a hint, not extra capacity.
        let mut r = Reservation::with_hold_capacity(5, -1, 2);
        let mut ids = SeqGen(0);
        let h = r.hold(2, &FixedClock(0), &mut ids).expect("2 of 5");
        assert_eq!(h.seats, 2);
        // Negative TTL clamped to 0 → expires_at == now (0): still active AT 0,
        // expired the instant after.
        assert_eq!(h.expires_at, 0, "ttl clamped to 0");
        assert_eq!(r.available(0), 3, "still held at the expiry second");
        assert_eq!(r.available(1), 5, "freed one second later");
    }

    #[test]
    fn hold_rejected_when_insufficient() {
        let mut r = Reservation::new(5, TTL);
        let clock = FixedClock(0);
        let mut ids = SeqGen(0);
        r.hold(4, &clock, &mut ids).expect("4 of 5");
        assert_eq!(
            r.hold(2, &clock, &mut ids),
            Err(HoldError::InsufficientAvailability)
        );
        // Exactly the remaining one still succeeds.
        r.hold(1, &clock, &mut ids).expect("last seat");
        assert_eq!(r.available(0), 0);
    }

    #[test]
    fn zero_seat_hold_is_rejected() {
        let mut r = Reservation::new(5, TTL);
        assert_eq!(
            r.hold(0, &FixedClock(0), &mut SeqGen(0)),
            Err(HoldError::ZeroSeats)
        );
    }

    #[test]
    fn confirm_books_seats_permanently() {
        let mut r = Reservation::new(10, TTL);
        let mut ids = SeqGen(0);
        let h = r.hold(4, &FixedClock(0), &mut ids).expect("hold");
        r.confirm(h.id, &FixedClock(10)).expect("confirm");
        assert_eq!(r.confirmed(), 4);
        // Confirmed seats stay booked even far past the original TTL.
        assert_eq!(r.available(10_000), 6);
        // Re-confirming the same id is now rejected (it is no longer live).
        assert_eq!(
            r.confirm(h.id, &FixedClock(10)),
            Err(ConfirmError::NotConfirmable)
        );
    }

    #[test]
    fn confirm_expired_hold_is_rejected() {
        let mut r = Reservation::new(10, TTL);
        let mut ids = SeqGen(0);
        let h = r.hold(4, &FixedClock(0), &mut ids).expect("hold");
        // TTL is 60; at 61 the hold has expired.
        assert_eq!(
            r.confirm(h.id, &FixedClock(61)),
            Err(ConfirmError::NotConfirmable)
        );
        assert_eq!(r.confirmed(), 0);
        assert_eq!(r.available(61), 10);
    }

    #[test]
    fn confirm_unknown_hold_is_rejected() {
        let mut r = Reservation::new(10, TTL);
        assert_eq!(
            r.confirm(999, &FixedClock(0)),
            Err(ConfirmError::NotConfirmable)
        );
    }

    #[test]
    fn release_returns_seats_and_is_idempotent() {
        let mut r = Reservation::new(10, TTL);
        let mut ids = SeqGen(0);
        let h = r.hold(4, &FixedClock(0), &mut ids).expect("hold");
        assert_eq!(r.available(0), 6);
        assert!(r.release(h.id, &FixedClock(1)), "first release removes it");
        assert_eq!(r.available(1), 10);
        // Idempotent: releasing again is a no-op that does not error.
        assert!(
            !r.release(h.id, &FixedClock(1)),
            "second release is a no-op"
        );
        assert_eq!(r.available(1), 10);
    }

    #[test]
    fn release_unknown_hold_is_noop() {
        let mut r = Reservation::new(10, TTL);
        assert!(!r.release(123, &FixedClock(0)));
    }

    #[test]
    fn expiry_frees_seats_lazily() {
        let mut r = Reservation::new(10, TTL);
        let mut ids = SeqGen(0);
        r.hold(10, &FixedClock(0), &mut ids).expect("hold all");
        assert_eq!(r.available(0), 0);
        // Before TTL: still held. At/after TTL+1: freed lazily on next query.
        assert_eq!(r.available(60), 0, "valid at expires_at");
        assert_eq!(r.available(61), 10, "freed after expiry");
    }

    #[test]
    fn expired_hold_does_not_block_new_hold() {
        let mut r = Reservation::new(10, TTL);
        let mut ids = SeqGen(0);
        r.hold(10, &FixedClock(0), &mut ids).expect("hold all");
        // The expired hold is reclaimed, so a fresh hold for all 10 succeeds.
        let h2 = r.hold(10, &FixedClock(61), &mut ids).expect("re-hold");
        assert_eq!(r.held(61), 10);
        assert_eq!(h2.seats, 10);
    }
}
