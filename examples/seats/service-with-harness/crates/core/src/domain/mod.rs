//! Pure domain for the seat-reservation service. No IO, no async.
//!
//! - [`hold`] — the hold lifecycle as a pure FSM (`fn next`); `dhx regen`
//!   generates the TLA+ lifecycle spec from it.
//! - [`reservation`] — the capacity/hold/confirm/release/expiry logic, behind
//!   the Clock/IdGen ports. The capacity invariant (REQ-005) holds by
//!   construction.
//! - [`proofs`] — `#[cfg(kani)]` harnesses proving the no-overbooking invariant.
pub mod hold;
pub mod reservation;

#[cfg(test)]
mod properties;

#[cfg(kani)]
mod proofs;
