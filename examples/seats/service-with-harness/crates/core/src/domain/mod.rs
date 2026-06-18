//! Pure domain for the seat-reservation service. No IO, no async.
//!
//! - [`seats`] — the capacity ledger ([`seats::SeatMap`]): hold / confirm /
//!   release / available / expiry, with the no-overbooking invariant.
//! - [`hold`] — the per-hold lifecycle FSM (`fn next`), projected to TLA+ by
//!   `dhx regen` via the `[fsm]` section of `harness.toml`.
//! - [`proofs`] — the `#[cfg(kani)]` no-overbooking proof harness.
pub mod hold;
pub mod proofs;
pub mod seats;
