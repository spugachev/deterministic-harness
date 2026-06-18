//! Pure domain of the transfer ledger. No IO, no async — all the rules live
//! here as total, panic-free functions and an in-memory aggregate, so they are
//! provable by Kani/proptest and replayable by DST.
//!
//! - [`parse`] — the untrusted single-line `TRANSFER …` command parser (fuzzed).
//! - [`money`] — the scalar transfer step (conservation / no-overdraft; Kani).
//! - [`lifecycle`] — the account `Open → Frozen → Closed` FSM (`fn next`; TLA+).
//! - [`ledger`] — the stateful aggregate tying balances + lifecycle + idempotency.
pub mod ledger;
pub mod lifecycle;
pub mod money;
pub mod parse;
