//! `seats` service — the outer IO crate.
//!
//! All non-determinism (wall clock, id generation) is wired here behind the
//! core's `Clock`/`IdGen` ports; the verified core stays IO-free. Concurrency
//! safety (REQ-007) comes from serializing every operation behind one lock over
//! the `SeatMap` ([`state::AppState`], ADR-0002).

#![forbid(unsafe_code)]

pub mod adapters;
pub mod http;
pub mod state;
