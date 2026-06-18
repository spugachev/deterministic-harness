//! `ledger` verified core — the pure transfer-ledger domain + ports. Zero IO.
//!
//! This crate has no async runtime, no database, no HTTP. All non-determinism is
//! abstracted behind [`ports`] (Clock/Rng/IdGen), which is what makes the
//! harness's DST/Loom/TSAN gates meaningful and lets Kani/proptest prove the
//! domain functions total. Concurrency (a `Mutex` around the [`domain::ledger`])
//! and HTTP live in the OUTER `api` crate behind these ports — never here.
//!
//! `domain/` holds the whole money-transfer ruleset: the untrusted command
//! parser, the scalar money step (conservation / no-overdraft), the account
//! lifecycle FSM, and the stateful ledger aggregate.

#![forbid(unsafe_code)]

pub mod domain;
pub mod ports;
