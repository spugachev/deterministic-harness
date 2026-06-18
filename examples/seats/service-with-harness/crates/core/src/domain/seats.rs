//! The seat-reservation ledger — the pure capacity domain.
//!
//! A [`SeatMap`] tracks an event with a fixed `capacity` of seats. Clients
//! *hold* seats (granting a TTL'd, id'd reservation), then *confirm* (booking
//! them permanently) or *release* (returning them) before the hold expires. A
//! hold not confirmed within its TTL frees its seats lazily, the next time the
//! map is consulted with a clock past the expiry.
//!
//! THE invariant — never overbooked: `confirmed + live_held(now) <= capacity`
//! under any sequence of operations, given a monotonically non-decreasing clock
//! (see ADR-0001). All arithmetic is checked/saturating and the type is
//! panic-free by construction, so it satisfies clippy's arithmetic restriction
//! and is a natural proptest/Kani target.
//!
//! The store is a flat `Vec<Hold>`, deliberately NOT a `BTreeMap`: a bounded
//! `for h in &slice` loop is tractable for Kani, whereas CBMC cannot bound a
//! symbolic map's tree-navigation (see CLAUDE.md "Kani proof").

/// Why a hold operation could not be granted or confirmed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldError {
    /// Fewer seats are free than were requested.
    InsufficientAvailability,
    /// The hold id is unknown, already confirmed, or has expired.
    NotHeld,
    /// A zero-seat request is meaningless.
    ZeroSeats,
}

/// A live, unconfirmed reservation: `seats` seats held under `id` until
/// `expires_at` (whole seconds since the Unix epoch).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hold {
    /// Unique hold id (minted by an `IdGen` port in the outer layer).
    pub id: u64,
    /// How many seats this hold reserves.
    pub seats: u32,
    /// Expiry instant — the hold is live while `now < expires_at`.
    pub expires_at: i64,
}

/// The seat ledger for a single event.
#[derive(Clone, Debug)]
pub struct SeatMap {
    capacity: u32,
    confirmed: u32,
    holds: Vec<Hold>,
}

/// Pure capacity check used by [`SeatMap::hold`] — the scalar heart of the
/// no-overbooking invariant, factored out so Kani can prove it EXHAUSTIVELY
/// over scalar inputs (no symbolic collection). Given `occupied` seats already
/// taken (confirmed + live holds), grant `req` more only if the new total fits
/// within `capacity`; returns the new occupancy, or `None` if it would oversell
/// or overflow.
#[must_use]
pub fn grant(capacity: u32, occupied: u32, req: u32) -> Option<u32> {
    match occupied.checked_add(req) {
        Some(total) if total <= capacity => Some(total),
        _ => None,
    }
}

impl SeatMap {
    /// A fresh event with `capacity` free seats.
    #[must_use]
    pub const fn new(capacity: u32) -> Self {
        Self {
            capacity,
            confirmed: 0,
            holds: Vec::new(),
        }
    }

    /// The event's total capacity.
    #[must_use]
    pub const fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Seats reserved by holds still live at `now` (expired holds excluded).
    #[must_use]
    pub fn live_held(&self, now: i64) -> u32 {
        let mut sum: u32 = 0;
        for h in &self.holds {
            if now < h.expires_at {
                sum = sum.saturating_add(h.seats);
            }
        }
        sum
    }

    /// Seats occupied right now: permanently confirmed plus still-live holds.
    #[must_use]
    pub fn occupied(&self, now: i64) -> u32 {
        self.confirmed.saturating_add(self.live_held(now))
    }

    /// Seats currently available to a new hold (REQ-006).
    #[must_use]
    pub fn available(&self, now: i64) -> u32 {
        self.capacity.saturating_sub(self.occupied(now))
    }

    /// Drop holds that have expired at `now`, lazily freeing their seats
    /// (REQ-005). Idempotent and called at the start of every mutator.
    fn purge_expired(&mut self, now: i64) {
        self.holds.retain(|h| now < h.expires_at);
    }

    /// Request a hold of `seats` seats, identified by `id`, expiring `ttl`
    /// seconds from `now` (REQ-001). Granted only if at least `seats` seats are
    /// free; otherwise rejected, never overbooking.
    ///
    /// # Errors
    /// - [`HoldError::ZeroSeats`] if `seats == 0`.
    /// - [`HoldError::InsufficientAvailability`] if fewer than `seats` are free.
    pub fn hold(&mut self, id: u64, seats: u32, now: i64, ttl: i64) -> Result<Hold, HoldError> {
        if seats == 0 {
            return Err(HoldError::ZeroSeats);
        }
        self.purge_expired(now);
        // The no-overbooking gate: occupancy + this request must fit capacity.
        if grant(self.capacity, self.occupied(now), seats).is_none() {
            return Err(HoldError::InsufficientAvailability);
        }
        let hold = Hold {
            id,
            seats,
            expires_at: now.saturating_add(ttl),
        };
        self.holds.push(hold);
        Ok(hold)
    }

    /// Confirm a live hold by `id`, booking its seats permanently (REQ-003).
    ///
    /// # Errors
    /// [`HoldError::NotHeld`] if the id is unknown, already confirmed/released,
    /// or has expired at `now`.
    pub fn confirm(&mut self, id: u64, now: i64) -> Result<u32, HoldError> {
        self.purge_expired(now);
        let idx = self
            .holds
            .iter()
            .position(|h| h.id == id)
            .ok_or(HoldError::NotHeld)?;
        let hold = self.holds.swap_remove(idx);
        self.confirmed = self.confirmed.saturating_add(hold.seats);
        Ok(hold.seats)
    }

    /// Release an unconfirmed hold by `id`, returning its seats (REQ-004).
    /// Releasing an unknown or already-expired hold is a no-op — this is
    /// idempotent and always succeeds.
    pub fn release(&mut self, id: u64, now: i64) {
        self.purge_expired(now);
        self.holds.retain(|h| h.id != id);
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "test-only assertions on known-good values"
    )]
    use super::*;

    const TTL: i64 = 120;

    #[test]
    fn grant_fits_within_capacity() {
        assert_eq!(grant(10, 3, 5), Some(8)); // fits
        assert_eq!(grant(10, 8, 5), None); // would oversell
        assert_eq!(grant(10, 10, 0), Some(10)); // exact boundary, no new seats
        assert_eq!(grant(u32::MAX, u32::MAX, 1), None); // overflow → None, not panic
    }

    #[test]
    fn hold_then_confirm_books_seats() {
        let mut m = SeatMap::new(10);
        let h = m.hold(1, 4, 0, TTL).unwrap();
        assert_eq!(h.seats, 4);
        assert_eq!(m.available(0), 6);
        assert_eq!(m.confirm(1, 1).unwrap(), 4);
        assert_eq!(m.available(1), 6); // still 6 free; the 4 are now booked
    }

    #[test]
    fn hold_rejected_when_insufficient() {
        let mut m = SeatMap::new(3);
        m.hold(1, 2, 0, TTL).unwrap();
        assert_eq!(
            m.hold(2, 2, 0, TTL),
            Err(HoldError::InsufficientAvailability)
        );
        m.hold(3, 1, 0, TTL).unwrap(); // exactly the last seat fits
        assert_eq!(m.available(0), 0);
    }

    #[test]
    fn zero_seat_hold_is_rejected() {
        let mut m = SeatMap::new(3);
        assert_eq!(m.hold(1, 0, 0, TTL), Err(HoldError::ZeroSeats));
    }

    #[test]
    fn confirm_unknown_or_expired_fails() {
        let mut m = SeatMap::new(3);
        assert_eq!(m.confirm(99, 0), Err(HoldError::NotHeld)); // unknown
        m.hold(1, 1, 0, TTL).unwrap();
        assert_eq!(m.confirm(1, TTL + 1), Err(HoldError::NotHeld)); // expired
        assert_eq!(m.confirm(1, TTL + 2), Err(HoldError::NotHeld)); // and gone
    }

    #[test]
    fn confirm_twice_fails_second_time() {
        let mut m = SeatMap::new(3);
        m.hold(1, 1, 0, TTL).unwrap();
        assert_eq!(m.confirm(1, 1).unwrap(), 1);
        assert_eq!(m.confirm(1, 2), Err(HoldError::NotHeld));
    }

    #[test]
    fn release_is_idempotent_noop_when_unknown() {
        let mut m = SeatMap::new(3);
        m.hold(1, 2, 0, TTL).unwrap();
        assert_eq!(m.available(0), 1);
        m.release(1, 0);
        assert_eq!(m.available(0), 3); // seats returned
        m.release(1, 0); // releasing again — no-op
        m.release(99, 0); // unknown — no-op
        assert_eq!(m.available(0), 3);
    }

    #[test]
    fn expiry_frees_seats_lazily() {
        let mut m = SeatMap::new(3);
        m.hold(1, 3, 0, TTL).unwrap();
        assert_eq!(m.available(0), 0); // full while held
        assert_eq!(m.available(TTL - 1), 0); // still live one second before expiry
                                             // The hold is live while `now < expires_at` and `expires_at == TTL`, so
                                             // at exactly `now == TTL` it is already expired and its seats are freed.
        assert_eq!(m.available(TTL), 3);
    }

    #[test]
    fn confirmed_seats_survive_expiry_window() {
        let mut m = SeatMap::new(3);
        m.hold(1, 2, 0, TTL).unwrap();
        m.confirm(1, 1).unwrap();
        // Confirmed seats are permanent; the TTL no longer applies.
        assert_eq!(m.available(TTL + 1000), 1);
    }

    proptest::proptest! {
        // THE law (REQ-001): across any sequence of operations under a
        // monotonically non-decreasing clock, occupancy never exceeds capacity.
        #[test]
        fn never_oversells(
            capacity in 1_u32..=64,
            ops in proptest::collection::vec(
                (0_u8..4, 0_u64..8, 1_u32..=8, 0_i64..5, 1_i64..=10),
                0..60,
            ),
        ) {
            let mut m = SeatMap::new(capacity);
            let mut now: i64 = 0;
            for (kind, id, seats, dt, ttl) in ops {
                now = now.saturating_add(dt); // clock only moves forward
                match kind {
                    0 => { let _ = m.hold(id, seats, now, ttl); }
                    1 => { let _ = m.confirm(id, now); }
                    2 => m.release(id, now),
                    _ => { let _ = m.available(now); }
                }
                // Invariant must hold after every single operation.
                proptest::prop_assert!(m.occupied(now) <= capacity);
            }
        }

        // Availability is exactly capacity minus occupancy, never underflowing.
        #[test]
        fn available_is_complement_of_occupied(
            capacity in 0_u32..=64,
            seats in 0_u32..=128,
            now in 0_i64..100,
        ) {
            let mut m = SeatMap::new(capacity);
            if seats > 0 {
                let _ = m.hold(1, seats, now, 120);
            }
            proptest::prop_assert_eq!(
                m.available(now),
                capacity.saturating_sub(m.occupied(now))
            );
        }
    }
}
