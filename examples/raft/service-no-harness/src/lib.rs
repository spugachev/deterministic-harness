//! A minimal but real Raft-replicated key-value store, fully in-memory and
//! deterministic. See the module docs for each piece:
//!
//! - [`resp`]  — RESP command parser (untrusted bytes -> typed command).
//! - [`kv`]    — the KV state machine (pure `apply`).
//! - [`raft`]  — leader election + log replication core, plus an in-process
//!   cluster simulator.
//! - [`ports`] — Clock / Rng determinism ports.

pub mod kv;
pub mod ports;
pub mod raft;
pub mod resp;
