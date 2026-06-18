//! BDD suite — cucumber scenarios in `spec/features/*.feature`, the mandatory
//! EARS floor. Every REQ has at least one scenario here; each drives the pure
//! `domain::seats::SeatMap` ledger directly (no HTTP — the core is IO-free).
//! Run with `cargo test -p core --test bdd`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_docs_in_private_items,
    // Forced by cucumber's step macros: every step takes `&mut World` (even
    // read-only ones) and step bodies are `async` (even when synchronous).
    clippy::needless_pass_by_ref_mut,
    clippy::unused_async,
    reason = "test-only; cucumber macro requirements"
)]

use core::domain::seats::{SeatError, SeatMap};
use cucumber::{given, then, when, World as _};

/// Scenario state: the ledger plus the results of the last hold/confirm/release.
#[derive(cucumber::World, Debug)]
struct SeatWorld {
    map: SeatMap,
    last_hold: Option<Result<u64, SeatError>>,
    last_confirm: Option<Result<(), SeatError>>,
    last_release: Option<bool>,
}

impl Default for SeatWorld {
    fn default() -> Self {
        Self {
            map: SeatMap::new(0),
            last_hold: None,
            last_confirm: None,
            last_release: None,
        }
    }
}

#[given(regex = r"^an event with (\d+) seats$")]
async fn given_event(w: &mut SeatWorld, capacity: u32) {
    w.map = SeatMap::new(capacity);
}

#[when(regex = r"^a client holds (\d+) seats with id (\d+) at time (\d+) expiring at (\d+)$")]
async fn when_hold(w: &mut SeatWorld, seats: u32, id: u64, now: i64, expires_at: i64) {
    w.last_hold = Some(w.map.hold(id, seats, now, expires_at));
}

#[when(regex = r"^the client confirms hold (\d+) at time (\d+)$")]
async fn when_confirm(w: &mut SeatWorld, id: u64, now: i64) {
    w.last_confirm = Some(w.map.confirm(id, now));
}

#[when(regex = r"^the client releases hold (\d+) at time (\d+)$")]
async fn when_release(w: &mut SeatWorld, id: u64, now: i64) {
    w.last_release = Some(w.map.release(id, now));
}

#[then(regex = r"^the hold shall be granted$")]
async fn then_hold_granted(w: &mut SeatWorld) {
    assert!(
        matches!(w.last_hold, Some(Ok(_))),
        "expected a granted hold, got {:?}",
        w.last_hold
    );
}

#[then(regex = r"^the last hold shall be rejected for insufficient availability$")]
async fn then_hold_rejected_insufficient(w: &mut SeatWorld) {
    assert_eq!(
        w.last_hold,
        Some(Err(SeatError::InsufficientAvailability)),
        "expected insufficient-availability rejection"
    );
}

#[then(regex = r"^the last hold shall be rejected for zero seats$")]
async fn then_hold_rejected_zero(w: &mut SeatWorld) {
    assert_eq!(
        w.last_hold,
        Some(Err(SeatError::ZeroSeatsRequested)),
        "expected zero-seats rejection"
    );
}

#[then(regex = r"^the confirmation shall succeed$")]
async fn then_confirm_ok(w: &mut SeatWorld) {
    assert_eq!(
        w.last_confirm,
        Some(Ok(())),
        "expected confirmation to succeed"
    );
}

#[then(regex = r"^the confirmation shall be rejected as unknown$")]
async fn then_confirm_rejected(w: &mut SeatWorld) {
    assert_eq!(
        w.last_confirm,
        Some(Err(SeatError::UnknownHold)),
        "expected unknown-hold rejection"
    );
}

#[then(regex = r"^the release shall report a hold was freed$")]
async fn then_release_freed(w: &mut SeatWorld) {
    assert_eq!(w.last_release, Some(true), "expected a hold to be freed");
}

#[then(regex = r"^the release shall report no hold was freed$")]
async fn then_release_noop(w: &mut SeatWorld) {
    assert_eq!(w.last_release, Some(false), "expected a no-op release");
}

#[then(regex = r"^the available count at time (\d+) shall be (\d+)$")]
async fn then_available(w: &mut SeatWorld, now: i64, expected: u32) {
    assert_eq!(w.map.available(now), expected);
}

#[then(regex = r"^the confirmed count shall be (\d+)$")]
async fn then_confirmed(w: &mut SeatWorld, expected: u32) {
    assert_eq!(w.map.confirmed(), expected);
}

#[then(regex = r"^confirmed plus held at time (\d+) shall not exceed (\d+)$")]
async fn then_invariant(w: &mut SeatWorld, now: i64, capacity: u32) {
    let used = w.map.confirmed().saturating_add(w.map.live_held(now));
    assert!(used <= capacity, "oversold: {used} > {capacity}");
}

fn main() {
    // NB: this crate is named `core`, which shadows std's `core` — so we cannot
    // use `#[tokio::main]` (its expansion references `core::future::…` and would
    // resolve here). Build the runtime explicitly instead.
    let rt = tokio::runtime::Runtime::new().expect("build tokio runtime");
    rt.block_on(SeatWorld::run("../../spec/features"));
}
