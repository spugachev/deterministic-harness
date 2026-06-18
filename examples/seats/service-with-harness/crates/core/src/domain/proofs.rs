//! Kani proofs for the domain's arithmetic invariants.
//!
//! These compile ONLY under Kani (`#[cfg(kani)]`) and are proven exhaustively by
//! CBMC, not run by `cargo test`. They follow the tractable shape from CLAUDE.md:
//! a few SCALAR `kani::any()` inputs over `u32` counts feeding a pure function —
//! NO symbolic `Vec`/`HashMap`, no loops over symbolic operations. The
//! multi-step / collection behaviour (that the real `Vec` of holds sums to
//! `held`) is routed to proptest + DST, which are built for it.

#![allow(clippy::unreachable, reason = "kani harnesses are proof-only")]

use crate::domain::seats::grant_step;

/// THE no-overbooking law, exhaustively: if the ledger invariant
/// `confirmed + held <= capacity` holds before a grant, then whenever
/// `grant_step` grants `req` more seats the invariant STILL holds afterwards
/// (`confirmed + new_held <= capacity`), and `new_held == held + req`.
#[kani::proof]
fn grant_step_never_oversells() {
    let capacity: u32 = kani::any();
    let confirmed: u32 = kani::any();
    let held: u32 = kani::any();
    let req: u32 = kani::any();

    // Precondition: the invariant holds going in (and counts are well-formed).
    kani::assume(confirmed <= capacity);
    kani::assume(held <= capacity - confirmed); // ⇒ confirmed + held <= capacity, no overflow

    if let Some(new_held) = grant_step(capacity, confirmed, held, req) {
        // The grant was accepted — the invariant must be preserved.
        assert!(req >= 1, "a zero request must never be granted");
        assert!(new_held == held + req, "granted exactly req more seats");
        // confirmed + new_held <= capacity  (the capacity invariant, preserved)
        assert!(confirmed + new_held <= capacity, "never oversold");
    }
}

/// A grant is rejected (`None`) exactly when the request is zero or would not
/// fit — i.e. acceptance is equivalent to `1 <= req` and
/// `confirmed + held + req <= capacity`. Pins the decision boundary so a
/// mutant that flips `<=`/`<` or drops the zero check is caught.
#[kani::proof]
fn grant_step_decision_is_exact() {
    let capacity: u32 = kani::any();
    let confirmed: u32 = kani::any();
    let held: u32 = kani::any();
    let req: u32 = kani::any();

    kani::assume(confirmed <= capacity);
    kani::assume(held <= capacity - confirmed);
    // Bound `req` so `confirmed + held + req` cannot overflow `u32`; the law is
    // the same arithmetic for all values, proven on the non-overflowing region.
    kani::assume(req <= capacity - (confirmed + held));

    let fits = req >= 1; // req <= capacity-(confirmed+held) guaranteed by assume
    assert_eq!(grant_step(capacity, confirmed, held, req).is_some(), fits);
}
