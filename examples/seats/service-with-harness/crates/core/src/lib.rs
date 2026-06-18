//! `seats` verified core — the pure seat-reservation domain + ports. Zero IO.
//!
//! This crate has no async runtime, no database, no HTTP. All non-determinism is
//! abstracted behind [`ports`] (Clock/Rng/IdGen), which is what makes the
//! harness's DST/Loom/TSAN gates meaningful and lets Kani/proptest prove the
//! domain functions total. Add IO adapters in OUTER crates behind these ports —
//! never here (the axum HTTP layer lives in `crates/api`).
//!
//! [`domain`] holds the seat-reservation logic: the hold lifecycle as a pure FSM
//! (`domain::hold::next`, from which `dhx regen` generates the TLA+ spec) and the
//! capacity/hold/confirm/release/expiry aggregate (`domain::reservation`). The
//! no-overbooking invariant (REQ-005) holds by construction and is proven by the
//! `#[cfg(kani)]` harnesses.

#![forbid(unsafe_code)]

pub mod domain;
pub mod ports;
