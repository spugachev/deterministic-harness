//! The seat-capacity ledger — the heart of the domain.
//!
//! A [`SeatMap`] tracks, for a single event of fixed `capacity`:
//! - permanently `confirmed` seats, and
//! - outstanding `holds` (each a `(id, seats, expires_at)` tuple).
//!
//! The capacity invariant — `confirmed + live-held <= capacity` — must hold under
//! ANY sequence of operations (REQ-005). All arithmetic is checked/saturating and
//! panic-free, so clippy's arithmetic restriction is satisfied and Kani/proptest
//! can prove the functions total.
//!
//! Time enters ONLY as an `i64` Unix-second argument that the caller reads from a
//! [`crate::ports::Clock`]; the ledger itself touches no wall-clock. Expiry is
//! lazy: a hold is "live" at instant `now` iff `now < expires_at`, so an expired
//! hold frees its seats the moment any operation observes the later time.
//!
//! NB: the hold store is a flat `Vec<Hold>`, deliberately NOT a `BTreeMap` — a
//! `for h in &holds` loop is bounded by length, which keeps the Kani proofs
//! tractable (a map's internal tree-navigation loops do not bound cleanly).

/// A single outstanding hold on `seats` seats, expiring at `expires_at`
/// (Unix seconds). Identified by a unique `id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hold {
    /// Unique hold identifier (from an [`crate::ports::IdGen`]).
    pub id: u64,
    /// Number of seats this hold reserves.
    pub seats: u32,
    /// Instant (Unix seconds) at which the hold expires; live iff `now < expires_at`.
    pub expires_at: i64,
}

impl Hold {
    /// Is this hold still live at instant `now`? (lazy expiry)
    #[must_use]
    pub const fn is_live(&self, now: i64) -> bool {
        now < self.expires_at
    }
}

/// Why an operation on the ledger could not be performed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatError {
    /// Fewer seats are free than were requested.
    InsufficientAvailability,
    /// Zero seats were requested (a hold must reserve at least one seat).
    ZeroSeatsRequested,
    /// No live hold with the given id exists (expired, unknown, or already confirmed).
    UnknownHold,
}

/// The no-overbooking arithmetic of a single hold, as a pure scalar step.
///
/// Given an event of `capacity` seats with `confirmed` booked and `held` live,
/// decide whether `req` more seats may be held. Returns the NEW live-held count
/// on success (`held + req`), or `None` if the request is invalid (`req == 0`)
/// or would not fit (`confirmed + held + req > capacity`).
///
/// This is the invariant-preserving STEP that [`SeatMap::hold`] performs on its
/// aggregate counts; isolating it as scalar arithmetic is what lets Kani prove
/// "never oversell" exhaustively (a symbolic `Vec` of holds would blow up CBMC).
/// The correspondence "the `Vec` of holds really sums to `held`" is covered by
/// the proptest sequence test, not Kani.
///
/// All arithmetic is checked: an overflowing sum is treated as "does not fit"
/// (returns `None`), so the function is total and panic-free.
#[must_use]
pub fn grant_step(capacity: u32, confirmed: u32, held: u32, req: u32) -> Option<u32> {
    if req == 0 {
        return None;
    }
    let used = confirmed.checked_add(held)?;
    let used_after = used.checked_add(req)?;
    if used_after <= capacity {
        // new live-held count
        held.checked_add(req)
    } else {
        None
    }
}

/// The seat ledger for a single fixed-capacity event.
#[derive(Clone, Debug)]
pub struct SeatMap {
    capacity: u32,
    confirmed: u32,
    holds: Vec<Hold>,
}

impl SeatMap {
    /// A fresh ledger for an event of `capacity` seats — nothing held or confirmed.
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

    /// Confirmed (permanently booked) seats.
    #[must_use]
    pub const fn confirmed(&self) -> u32 {
        self.confirmed
    }

    /// Seats reserved by holds that are still live at `now` (saturating sum).
    #[must_use]
    pub fn live_held(&self, now: i64) -> u32 {
        self.holds
            .iter()
            .filter(|h| h.is_live(now))
            .fold(0_u32, |acc, h| acc.saturating_add(h.seats))
    }

    /// Seats currently available to be held at `now`:
    /// `capacity - confirmed - live-held` (saturating, never underflows).
    #[must_use]
    pub fn available(&self, now: i64) -> u32 {
        self.capacity
            .saturating_sub(self.confirmed)
            .saturating_sub(self.live_held(now))
    }

    /// Try to place a hold for `seats` seats at instant `now`, expiring at
    /// `expires_at`, identified by `id`.
    ///
    /// Succeeds iff `seats >= 1` and `seats <= available(now)`; on success the
    /// hold is recorded and `id` returned. The capacity invariant is preserved
    /// because we only grant when the request fits in current availability.
    ///
    /// # Errors
    /// - [`SeatError::ZeroSeatsRequested`] if `seats == 0`.
    /// - [`SeatError::InsufficientAvailability`] if `seats > available(now)`.
    pub fn hold(
        &mut self,
        id: u64,
        seats: u32,
        now: i64,
        expires_at: i64,
    ) -> Result<u64, SeatError> {
        if seats == 0 {
            return Err(SeatError::ZeroSeatsRequested);
        }
        // The fit decision is the proven scalar step: it returns `Some` exactly
        // when `confirmed + live-held + seats <= capacity`.
        if grant_step(self.capacity, self.confirmed, self.live_held(now), seats).is_none() {
            return Err(SeatError::InsufficientAvailability);
        }
        self.holds.push(Hold {
            id,
            seats,
            expires_at,
        });
        Ok(id)
    }

    /// Confirm a live hold by `id` at instant `now`: its seats become permanently
    /// booked and the hold is removed.
    ///
    /// # Errors
    /// [`SeatError::UnknownHold`] if no hold with `id` is live at `now`
    /// (expired, unknown, or already confirmed/released).
    pub fn confirm(&mut self, id: u64, now: i64) -> Result<(), SeatError> {
        let idx = self
            .holds
            .iter()
            .position(|h| h.id == id && h.is_live(now))
            .ok_or(SeatError::UnknownHold)?;
        // `idx` came from `position`, so indexing/removal is in-bounds.
        let h = self.holds.swap_remove(idx);
        // Confirmed seats can never exceed capacity: the hold was live and live
        // holds already counted against availability, so `confirmed + h.seats`
        // was a subset of `capacity`. Saturating add is a belt-and-braces guard.
        self.confirmed = self.confirmed.saturating_add(h.seats);
        Ok(())
    }

    /// Release an unconfirmed hold by `id`: its seats return to available.
    /// Idempotent — releasing an expired/unknown hold is a no-op (returns `false`).
    /// Returns `true` iff a hold was actually removed.
    pub fn release(&mut self, id: u64, now: i64) -> bool {
        // A live hold is released; an expired hold is already free, so dropping
        // it (or finding nothing) is the same observable no-op.
        if let Some(idx) = self.holds.iter().position(|h| h.id == id && h.is_live(now)) {
            self.holds.swap_remove(idx);
            true
        } else {
            false
        }
    }

    /// Drop every hold that has expired at `now` (lazy GC). Purely a
    /// space-reclaiming convenience: `available`/`live_held` already ignore
    /// expired holds, so this never changes any observable count.
    pub fn purge_expired(&mut self, now: i64) {
        self.holds.retain(|h| h.is_live(now));
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

    #[test]
    fn fresh_map_is_all_available() {
        let m = SeatMap::new(10);
        assert_eq!(m.available(0), 10);
        assert_eq!(m.confirmed(), 0);
        assert_eq!(m.capacity(), 10);
    }

    #[test]
    fn hold_reduces_availability_then_confirm_books() {
        let mut m = SeatMap::new(10);
        m.hold(1, 3, 0, 100).unwrap();
        assert_eq!(m.available(0), 7);
        assert_eq!(m.live_held(0), 3);
        m.confirm(1, 50).unwrap();
        assert_eq!(m.confirmed(), 3);
        assert_eq!(m.available(50), 7);
        assert_eq!(m.live_held(50), 0);
    }

    #[test]
    fn hold_rejects_when_insufficient() {
        let mut m = SeatMap::new(5);
        m.hold(1, 4, 0, 100).unwrap();
        assert_eq!(
            m.hold(2, 2, 0, 100),
            Err(SeatError::InsufficientAvailability)
        );
        // exactly the remaining 1 is allowed
        assert_eq!(m.hold(3, 1, 0, 100), Ok(3));
    }

    #[test]
    fn hold_rejects_zero_seats() {
        let mut m = SeatMap::new(5);
        assert_eq!(m.hold(1, 0, 0, 100), Err(SeatError::ZeroSeatsRequested));
    }

    #[test]
    fn expired_hold_frees_its_seats() {
        let mut m = SeatMap::new(5);
        m.hold(1, 5, 0, 100).unwrap();
        assert_eq!(m.available(50), 0); // still live before expiry
        assert_eq!(m.available(100), 5); // at expiry instant: freed (now < expires_at false)
        assert_eq!(m.available(200), 5); // long after
    }

    #[test]
    fn confirm_expired_hold_is_rejected() {
        let mut m = SeatMap::new(5);
        m.hold(1, 5, 0, 100).unwrap();
        assert_eq!(m.confirm(1, 100), Err(SeatError::UnknownHold));
        assert_eq!(m.confirm(1, 200), Err(SeatError::UnknownHold));
    }

    #[test]
    fn confirm_unknown_or_twice_is_rejected() {
        let mut m = SeatMap::new(5);
        m.hold(1, 2, 0, 100).unwrap();
        assert_eq!(m.confirm(99, 0), Err(SeatError::UnknownHold));
        m.confirm(1, 0).unwrap();
        assert_eq!(m.confirm(1, 0), Err(SeatError::UnknownHold)); // already confirmed
    }

    #[test]
    fn release_is_idempotent_and_frees_seats() {
        let mut m = SeatMap::new(5);
        m.hold(1, 3, 0, 100).unwrap();
        assert!(m.release(1, 0)); // first release frees
        assert_eq!(m.available(0), 5);
        assert!(!m.release(1, 0)); // second release: no-op
        assert!(!m.release(99, 0)); // unknown: no-op
    }

    #[test]
    fn release_of_expired_hold_is_noop() {
        let mut m = SeatMap::new(5);
        m.hold(1, 3, 0, 100).unwrap();
        assert!(!m.release(1, 200)); // already expired → no-op
    }

    #[test]
    fn purge_expired_does_not_change_counts() {
        let mut m = SeatMap::new(5);
        m.hold(1, 2, 0, 100).unwrap();
        m.hold(2, 1, 0, 300).unwrap();
        let before = m.available(200);
        m.purge_expired(200);
        assert_eq!(m.available(200), before);
        assert_eq!(m.live_held(200), 1); // only hold #2 remains live
    }

    proptest::proptest! {
        // The capacity invariant under a sequence of operations on a MONOTONIC
        // clock: confirmed + live-held never exceeds capacity, ever. Time only
        // moves forward (accumulated non-negative deltas) — an arbitrary clock
        // would let a confirmed-then-rewound hold double-count, which is
        // physically impossible.
        #[test]
        fn never_oversells_under_op_sequence(
            capacity in 0_u32..32,
            ops in proptest::collection::vec(
                (0_u32..4, 1_u32..8, 0_i64..5, 1_i64..6),
                0..40,
            ),
        ) {
            let mut m = SeatMap::new(capacity);
            let mut now: i64 = 0;
            let mut next_id: u64 = 1;
            let mut live_ids: Vec<u64> = Vec::new();
            for (kind, seats, dt, ttl) in ops {
                now = now.saturating_add(dt); // monotonic: never goes backwards
                match kind {
                    0 => {
                        // hold
                        let id = next_id;
                        next_id = next_id.saturating_add(1);
                        if m.hold(id, seats, now, now.saturating_add(ttl)).is_ok() {
                            live_ids.push(id);
                        }
                    }
                    1 => {
                        // confirm a known id (if any)
                        if let Some(&id) = live_ids.first() {
                            let _ = m.confirm(id, now);
                        }
                    }
                    2 => {
                        // release a known id (if any)
                        if let Some(&id) = live_ids.first() {
                            let _ = m.release(id, now);
                            live_ids.remove(0);
                        }
                    }
                    _ => {
                        // purge
                        m.purge_expired(now);
                    }
                }
                // THE invariant, checked after every operation.
                let used = m.confirmed().saturating_add(m.live_held(now));
                proptest::prop_assert!(
                    used <= capacity,
                    "oversold: confirmed {} + live_held {} > capacity {}",
                    m.confirmed(), m.live_held(now), capacity
                );
            }
        }

        // Availability is exactly capacity - confirmed - live_held (the
        // accounting identity), and never exceeds capacity.
        #[test]
        fn available_is_the_accounting_complement(
            capacity in 0_u32..32,
            id in 1_u64..100,
            seats in 1_u32..40,
            now in 0_i64..100,
            ttl in 1_i64..100,
        ) {
            let mut m = SeatMap::new(capacity);
            let _ = m.hold(id, seats, now, now.saturating_add(ttl));
            let avail = m.available(now);
            proptest::prop_assert!(avail <= capacity);
            let expect = capacity
                .saturating_sub(m.confirmed())
                .saturating_sub(m.live_held(now));
            proptest::prop_assert_eq!(avail, expect);
        }
    }
}
