//! `api` — the outer IO crate for the ledger service.
//!
//! Adds the two things the IO-free `core` deliberately lacks: concurrency (a
//! lock around the pure ledger, [`state::SharedLedger`]) and an HTTP surface
//! ([`http::router`], axum). No domain rule lives here; this crate only adapts
//! the verified core to threads and sockets.

#![forbid(unsafe_code)]

pub mod http;
pub mod state;
