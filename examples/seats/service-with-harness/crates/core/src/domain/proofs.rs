//! Kani harnesses for the no-overbooking invariant (REQ-005).
//!
//! These compile only under `cargo kani` (`#[cfg(kani)]`) and are excluded from
//! the mutation gate (they cannot be killed by `cargo test`). The harness drives
//! the real [`Reservation`] over symbolic inputs and asserts the capacity
//! invariant `confirmed + held(now) <= capacity` holds after every operation —
//! proving it for ALL inputs within the bound, not just the tested cases.

use crate::domain::reservation::Reservation;
use crate::ports::{FixedClock, SeqGen};

/// No sequence of hold/confirm/release operations can overbook (REQ-005).
///
/// Bounded so CBMC stays tractable: a tiny symbolic capacity and TWO symbolic
/// operations. The cost driver under Kani is the domain's `Vec<Hold>` — its
/// reallocation logic explodes the state space — so the harness pre-sizes the
/// reservation's backing store (`with_hold_capacity`) to the maximum number of
/// holds two operations can create, removing the `grow` reasoning entirely. Two
/// operations is enough to exercise every interaction (hold→confirm,
/// hold→release, hold→hold-over-capacity) against the invariant.
#[kani::proof]
#[kani::unwind(4)]
fn no_overbooking_under_operations() {
    let capacity: u32 = kani::any();
    kani::assume(capacity <= 3);
    let ttl: i64 = kani::any();
    kani::assume((0..=4).contains(&ttl));

    // Pre-reserve room for both possible holds so CBMC never reasons about Vec
    // reallocation (the memory blow-up). The invariant is unaffected.
    let mut r = Reservation::with_hold_capacity(capacity, ttl, 2);
    let mut ids = SeqGen(0);

    let mut last_hold_id: u64 = 0;
    let start: i64 = kani::any();
    kani::assume((0..=2).contains(&start));

    for step in 0..2_u32 {
        let now = start.saturating_add(i64::from(step));
        let clock = FixedClock(now);
        let action: u8 = kani::any();
        match action % 3 {
            0 => {
                let seats: u32 = kani::any();
                kani::assume(seats <= 4);
                if let Ok(h) = r.hold(seats, &clock, &mut ids) {
                    last_hold_id = h.id;
                    // A granted hold never reserves more than capacity.
                    assert!(h.seats <= capacity);
                }
            }
            1 => {
                let _ = r.confirm(last_hold_id, &clock);
            }
            _ => {
                let _ = r.release(last_hold_id, &clock);
            }
        }
        // The core invariant: confirmed + currently-held never exceeds capacity,
        // and the sum never overflows.
        let total = r.confirmed().checked_add(r.held(now));
        assert!(matches!(total, Some(t) if t <= capacity), "overbooking");
    }
}

/// `available` is exactly `capacity - confirmed - held` and never underflows —
/// a standalone proof of the availability accounting (REQ-006).
#[kani::proof]
#[kani::unwind(4)]
fn available_never_underflows() {
    let capacity: u32 = kani::any();
    kani::assume(capacity <= 3);
    let mut r = Reservation::new(capacity, 5);
    let mut ids = SeqGen(0);
    let seats: u32 = kani::any();
    kani::assume(seats <= 5);
    let clock = FixedClock(0);
    let _ = r.hold(seats, &clock, &mut ids);
    let avail = r.available(0);
    // available + confirmed + held == capacity exactly, with no wraparound.
    let sum = avail
        .checked_add(r.confirmed())
        .and_then(|s| s.checked_add(r.held(0)));
    assert!(sum == Some(capacity));
    assert!(avail <= capacity);
}
