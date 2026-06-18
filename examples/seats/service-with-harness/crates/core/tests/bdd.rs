//! BDD suite — cucumber scenarios in `spec/features/*.feature`, the mandatory
//! EARS floor. Every REQ has at least one scenario here; each drives the pure
//! seat-reservation domain directly (no HTTP needed — the core is IO-free) with
//! a fixed `Clock` and a seeded `IdGen`. Run with `cargo test -p core --test bdd`.
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

use core::domain::reservation::{ConfirmError, Hold, HoldError, Reservation};
use core::ports::{FixedClock, SeqGen};
use cucumber::{given, then, when, World as _};

/// Scenario state. The reservation and id counter are built by the setup
/// `Given` step; each hold attempt is recorded so a `Then` can assert about it
/// by seat count, and the last hold's id threads into confirm/release.
#[derive(cucumber::World, Debug, Default)]
struct SeatsWorld {
    now: i64,
    reservation: Option<Reservation>,
    /// Seed/counter for the deterministic `IdGen`, carried across holds.
    id_counter: u64,
    holds: Vec<(u32, Result<Hold, HoldError>)>,
    last_id: u64,
    confirm_result: Option<Result<(), ConfirmError>>,
    release_result: Option<bool>,
}

impl SeatsWorld {
    fn r(&mut self) -> &mut Reservation {
        self.reservation
            .as_mut()
            .expect("Background set up the event")
    }

    /// Perform a hold for `seats` at `now`, recording the result and (on
    /// success) the last hold id. `SeqGen` is the seeded `IdGen` port; its counter
    /// is carried across holds so every granted id is unique.
    fn do_hold(&mut self, seats: u32, now: i64) {
        let mut g = SeqGen(self.id_counter);
        let clock = FixedClock(now);
        let result = self.r().hold(seats, &clock, &mut g);
        self.id_counter = g.0;
        if let Ok(h) = &result {
            self.last_id = h.id;
        }
        self.holds.push((seats, result));
    }
}

#[given(regex = r"^an event with capacity (\d+) and a hold TTL of (\d+) seconds$")]
async fn given_event(w: &mut SeatsWorld, capacity: u32, ttl: i64) {
    w.reservation = Some(Reservation::new(capacity, ttl));
}

#[given(regex = r"^the current time is (\d+)$")]
async fn given_now(w: &mut SeatsWorld, now: i64) {
    w.now = now;
}

#[given(regex = r"^a client holds (\d+) seats$")]
#[when(regex = r"^a client holds (\d+) seats$")]
async fn holds(w: &mut SeatsWorld, seats: u32) {
    let now = w.now;
    w.do_hold(seats, now);
}

#[when(regex = r"^the time advances to (\d+)$")]
async fn time_advances(w: &mut SeatsWorld, now: i64) {
    w.now = now;
}

#[given(regex = r"^the client confirms the hold at time (\d+)$")]
#[when(regex = r"^the client confirms the hold at time (\d+)$")]
async fn confirms(w: &mut SeatsWorld, at: i64) {
    let id = w.last_id;
    let res = w.r().confirm(id, &FixedClock(at));
    w.confirm_result = Some(res);
}

#[when(regex = r"^the client releases the hold at time (\d+)$")]
async fn releases(w: &mut SeatsWorld, at: i64) {
    let id = w.last_id;
    let removed = w.r().release(id, &FixedClock(at));
    w.release_result = Some(removed);
}

#[when(regex = r"^the client releases hold id (\d+) at time (\d+)$")]
async fn releases_id(w: &mut SeatsWorld, id: u64, at: i64) {
    let removed = w.r().release(id, &FixedClock(at));
    w.release_result = Some(removed);
}

#[then(regex = r"^the system shall grant (?:a|the) hold for (\d+) seats$")]
async fn then_granted(w: &mut SeatsWorld, seats: u32) {
    let ok = w
        .holds
        .iter()
        .any(|(req, res)| *req == seats && matches!(res, Ok(h) if h.seats == seats));
    assert!(
        ok,
        "expected a granted hold for {seats} seats; have {:?}",
        w.holds
    );
}

#[then(regex = r"^the system shall reject the hold for insufficient availability$")]
async fn then_rejected_insufficient(w: &mut SeatsWorld) {
    let ok = w
        .holds
        .iter()
        .any(|(_, res)| matches!(res, Err(HoldError::InsufficientAvailability)));
    assert!(
        ok,
        "expected an insufficient-availability rejection; have {:?}",
        w.holds
    );
}

#[then(regex = r"^the system shall reject the hold for zero seats$")]
async fn then_rejected_zero(w: &mut SeatsWorld) {
    let ok = w
        .holds
        .iter()
        .any(|(_, res)| matches!(res, Err(HoldError::ZeroSeats)));
    assert!(ok, "expected a zero-seats rejection; have {:?}", w.holds);
}

#[then(regex = r"^the system shall report (\d+) seats available$")]
async fn then_available(w: &mut SeatsWorld, expected: u32) {
    let now = w.now;
    assert_eq!(w.r().available(now), expected);
}

#[then(regex = r"^the system shall report (\d+) seats confirmed$")]
async fn then_confirmed(w: &mut SeatsWorld, expected: u32) {
    assert_eq!(w.r().confirmed(), expected);
}

#[then(regex = r"^the system shall report the confirmation succeeded$")]
async fn then_confirm_ok(w: &mut SeatsWorld) {
    assert_eq!(w.confirm_result, Some(Ok(())));
}

#[then(regex = r"^the system shall reject the confirmation$")]
async fn then_confirm_rejected(w: &mut SeatsWorld) {
    assert!(
        matches!(w.confirm_result, Some(Err(_))),
        "expected a rejected confirmation"
    );
}

#[then(regex = r"^the system shall report the release was a no-op$")]
async fn then_release_noop(w: &mut SeatsWorld) {
    assert_eq!(w.release_result, Some(false));
}

#[then(regex = r"^the system shall keep confirmed plus held seats at most (\d+)$")]
async fn then_capacity_held(w: &mut SeatsWorld, cap: u32) {
    let now = w.now;
    let total = w.r().confirmed().saturating_add(w.r().held(now));
    assert!(total <= cap, "overbooking: {total} > {cap}");
}

fn main() {
    // NB: this crate is named `core`, which shadows std's `core` — so we cannot
    // use `#[tokio::main]` (its expansion references `core::future::…` and would
    // resolve here). Build the runtime explicitly instead.
    //
    // Path is relative to this crate (crates/core); specs are centralized at the
    // workspace root in spec/features/.
    let rt = tokio::runtime::Runtime::new().expect("build tokio runtime");
    rt.block_on(SeatsWorld::run("../../spec/features"));
}
