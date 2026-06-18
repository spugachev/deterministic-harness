//! Pure domain. No IO, no async — the verified core of the seat-reservation
//! service.
//!
//! - [`seats`] — the seat-capacity ledger (`SeatMap`) and its no-overbooking
//!   scalar step (`grant_step`).
//! - [`hold`] — the per-hold lifecycle FSM (`fn next`), projected to TLA+ by
//!   `dhx regen` via `harness.toml`'s `[fsm]` section.
//! - `proofs` — `#[cfg(kani)]` exhaustive proofs of the arithmetic invariants.
pub mod hold;
pub mod seats;

#[cfg(kani)]
mod proofs;
