//! `api` — the outer IO crate for the seat-reservation service.
//!
//! It wires the IO-free [`core`] domain to the outside world: an axum HTTP layer
//! ([`app`]) and the production port adapters ([`adapters`], where real
//! wall-clock time enters the system). The core never depends on this crate, so
//! all the proven safety logic stays in `core`; this crate only translates
//! between HTTP and the domain.

#![forbid(unsafe_code)]

pub mod adapters;
pub mod app;
