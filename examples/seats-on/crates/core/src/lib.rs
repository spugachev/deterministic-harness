//! `seats` verified core — pure domain + ports. Zero IO.
//!
//! This crate has no async runtime, no database, no HTTP. All non-determinism
//! is abstracted behind [`ports`] (Clock/Rng/IdGen), which is exactly what
//! makes the harness's DST/Loom/TSAN gates meaningful and Kani/Verus/proptest
//! able to prove the domain functions total. Add IO adapters in OUTER crates
//! behind these ports — never here.

#![forbid(unsafe_code)]

pub mod domain;
pub mod ports;

pub use domain::capacity::{reserve, OverError};
pub use domain::hold::{is_expired, Hold};
pub use domain::parse::{parse_booking, Booking, ParseError};
pub use domain::state::{next, SeatEvent, SeatState};
