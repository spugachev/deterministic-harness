//! `seats` HTTP/IO layer — the OUTER crate.
//!
//! All IO lives here, behind the core's ports: the axum HTTP surface
//! ([`http`]), the real wall-clock / id adapters ([`adapters`]), and the
//! mutex-serialized service ([`service`]) that turns the single-threaded,
//! IO-free [`core::domain::seats::SeatMap`] into something safe to share across
//! concurrent requests (ADR-0002). The verified core never depends on this crate.

#![forbid(unsafe_code)]

pub mod adapters;
pub mod http;
pub mod service;
