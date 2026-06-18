//! Property tests (proptest) for the seat-reservation laws — the cheap
//! workhorse that complements the bounded Kani proofs (REQ-005/REQ-006).
//!
//! These live inline in the crate (a `#[cfg(test)]` module) rather than in
//! `tests/` because this crate is named `core`: an EXTERNAL integration test
//! would make proptest's generated `::core::result::…` paths resolve to this
//! crate instead of std and fail to compile. Inline, `core` resolves to std as
//! the macros expect.
//!
//! The headline law is the capacity invariant: under ANY random sequence of
//! hold/confirm/release/expiry operations against a fixed-capacity event,
//! `confirmed + currently_held` never exceeds `capacity`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test-only"
)]

use crate::domain::reservation::Reservation;
use crate::ports::{FixedClock, SeqGen};
use proptest::prelude::*;

/// One operation the model can apply.
#[derive(Clone, Debug)]
enum Op {
    Hold(u32),
    Confirm,
    Release,
    Advance(i64),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0_u32..6).prop_map(Op::Hold),
        Just(Op::Confirm),
        Just(Op::Release),
        (0_i64..50).prop_map(Op::Advance),
    ]
}

proptest! {
    /// The capacity invariant holds after every operation in any sequence.
    #[test]
    fn capacity_invariant_never_overbooks(
        capacity in 0_u32..20,
        ttl in 0_i64..30,
        ops in prop::collection::vec(op_strategy(), 0..40),
    ) {
        let mut r = Reservation::new(capacity, ttl);
        let mut ids = SeqGen(0);
        let mut now = 0_i64;
        let mut last_id = 0_u64;

        for op in ops {
            match op {
                Op::Hold(seats) => {
                    if let Ok(h) = r.hold(seats, &FixedClock(now), &mut ids) {
                        last_id = h.id;
                    }
                }
                Op::Confirm => {
                    let _ = r.confirm(last_id, &FixedClock(now));
                }
                Op::Release => {
                    let _ = r.release(last_id, &FixedClock(now));
                }
                Op::Advance(d) => {
                    now = now.saturating_add(d);
                }
            }
            // The invariant: confirmed + currently-held <= capacity, no overflow.
            let total = r.confirmed().checked_add(r.held(now));
            prop_assert!(matches!(total, Some(t) if t <= capacity));
        }
    }

    /// `available` equals `capacity - confirmed - held` exactly and never
    /// underflows (REQ-006).
    #[test]
    fn availability_accounting_is_exact(
        capacity in 0_u32..20,
        seats in 0_u32..25,
        now in 0_i64..100,
    ) {
        let mut r = Reservation::new(capacity, 60);
        let mut ids = SeqGen(0);
        let _ = r.hold(seats, &FixedClock(now), &mut ids);
        let avail = r.available(now);
        let sum = avail
            .checked_add(r.confirmed())
            .and_then(|s| s.checked_add(r.held(now)));
        prop_assert_eq!(sum, Some(capacity));
        prop_assert!(avail <= capacity);
    }

    /// Release is idempotent: releasing the same id twice removes it once and
    /// leaves availability unchanged after the first (REQ-003).
    #[test]
    fn release_is_idempotent(
        capacity in 1_u32..20,
        seats in 1_u32..20,
    ) {
        let mut r = Reservation::new(capacity, 60);
        let mut ids = SeqGen(0);
        if let Ok(h) = r.hold(seats, &FixedClock(0), &mut ids) {
            let first = r.release(h.id, &FixedClock(1));
            let avail_after_first = r.available(1);
            let second = r.release(h.id, &FixedClock(1));
            prop_assert!(first);
            prop_assert!(!second);
            prop_assert_eq!(avail_after_first, r.available(1));
        }
    }
}
