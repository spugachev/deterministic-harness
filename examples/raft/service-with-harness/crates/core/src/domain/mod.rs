//! Pure domain. No IO, no async — YOUR types and logic live here.
//!
//! `raftkv` is a Raft-replicated key-value store, all simulated in one process:
//!
//! * [`resp`] — the RESP command parser (untrusted bytes → typed [`resp::Command`]),
//!   a pure, total, panic-free function (fuzzed for panic-freedom).
//! * [`kv`] — the deterministic key-value state machine that committed commands
//!   apply to.
//! * [`raft`] — the pure, deterministic Raft consensus core (role FSM, scalar
//!   safety decisions, replicated log, node driver).
//!
//! All non-determinism (time, randomness) flows through [`crate::ports`], so a
//! whole cluster run is reproducible from a seed.
pub mod kv;
pub mod raft;
pub mod resp;
