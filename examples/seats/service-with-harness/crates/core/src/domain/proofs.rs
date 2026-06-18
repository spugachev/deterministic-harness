//! Kani proofs for the no-overbooking invariant (REQ-001).
//!
//! These compile ONLY under `cfg(kani)`. They follow the tractable shape from
//! CLAUDE.md: prove the invariant-preserving STEP over a few scalar
//! `kani::any()` inputs and pure arithmetic — NO symbolic `Vec`/`HashMap` and
//! no loops over symbolic state — so CBMC never runs out of memory. The
//! collection/multi-step behaviour is left to proptest + DST, which exercise
//! the long operation sequences the bounded proof cannot.

#![cfg(kani)]

use super::seats::grant;

/// `grant` never lets occupancy exceed capacity: if it returns a new total,
/// that total is `<= capacity`. Proven EXHAUSTIVELY over every scalar triple
/// (not sampled like proptest).
#[kani::proof]
fn grant_never_oversells() {
    let capacity: u32 = kani::any();
    let occupied: u32 = kani::any();
    let req: u32 = kani::any();
    // Precondition: we never start already-oversold.
    kani::assume(occupied <= capacity);
    if let Some(total) = grant(capacity, occupied, req) {
        assert!(total <= capacity); // invariant preserved by the operation
        assert!(total >= occupied); // a grant only ever adds seats
    }
}

/// `grant` is monotone and overflow-safe: a request that fits leaves capacity
/// headroom, and an impossible (overflowing or oversized) request is rejected
/// with `None` rather than panicking.
#[kani::proof]
fn grant_rejects_when_no_room() {
    let capacity: u32 = kani::any();
    let occupied: u32 = kani::any();
    let req: u32 = kani::any();
    match grant(capacity, occupied, req) {
        Some(total) => {
            // Granted ⇒ the exact arithmetic held with room to spare.
            assert!(total == occupied + req);
            assert!(total <= capacity);
        }
        None => {
            // Rejected ⇒ either it would overflow, or it would exceed capacity.
            let overflows = occupied.checked_add(req).is_none();
            let oversells = occupied.checked_add(req).is_some_and(|t| t > capacity);
            assert!(overflows || oversells);
        }
    }
}
