//! BDD suite — cucumber scenarios in `spec/features/*.feature`, the mandatory
//! EARS floor. Every REQ has at least one scenario here; each drives the pure
//! seat domain directly (no HTTP needed — the core is IO-free). Run with
//! `cargo test -p core --test bdd`.
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

use core::domain::hold::{next, Event, State};
use core::domain::seats::{HoldError, SeatMap};
use cucumber::{given, then, when, World as _};

/// Fixed TTL (seconds) used by the scenarios — mirrors the production default.
const TTL: i64 = 120;

/// Scenario state: the ledger, a logical clock, and the last operation result.
#[derive(cucumber::World, Debug)]
struct SeatWorld {
    map: SeatMap,
    now: i64,
    last_hold: Result<u64, HoldError>,
    last_confirm: Result<u32, HoldError>,
    /// FSM scenarios: the current lifecycle state and last transition outcome.
    fsm_state: State,
    fsm_after: Option<State>,
}

impl Default for SeatWorld {
    fn default() -> Self {
        Self {
            map: SeatMap::new(0),
            now: 0,
            last_hold: Err(HoldError::NotHeld),
            last_confirm: Err(HoldError::NotHeld),
            fsm_state: State::Held,
            fsm_after: None,
        }
    }
}

#[given(regex = r"^a venue with (\d+) seats$")]
async fn given_venue(w: &mut SeatWorld, capacity: u32) {
    w.map = SeatMap::new(capacity);
    w.now = 0;
}

#[given(regex = r"^a hold (\d+) for (\d+) seats$")]
async fn given_hold(w: &mut SeatWorld, id: u64, seats: u32) {
    w.map
        .hold(id, seats, w.now, TTL)
        .expect("setup hold must succeed");
}

#[given(regex = r"^the time advances past the hold TTL$")]
async fn given_time_past_ttl(w: &mut SeatWorld) {
    w.now = w.now.saturating_add(TTL).saturating_add(1);
}

#[when(regex = r"^a client requests a hold (\d+) for (\d+) seats$")]
async fn when_request_hold(w: &mut SeatWorld, id: u64, seats: u32) {
    w.last_hold = w.map.hold(id, seats, w.now, TTL).map(|h| h.id);
}

#[when(regex = r"^a client confirms hold (\d+)$")]
async fn when_confirm(w: &mut SeatWorld, id: u64) {
    w.last_confirm = w.map.confirm(id, w.now);
}

#[when(regex = r"^a client releases hold (\d+)$")]
async fn when_release(w: &mut SeatWorld, id: u64) {
    w.map.release(id, w.now);
}

#[then(regex = r"^the service shall grant the hold$")]
async fn then_granted(w: &mut SeatWorld) {
    assert!(w.last_hold.is_ok(), "expected grant, got {:?}", w.last_hold);
}

#[then(regex = r"^the service shall reject the hold$")]
async fn then_rejected(w: &mut SeatWorld) {
    assert_eq!(
        w.last_hold,
        Err(HoldError::InsufficientAvailability),
        "expected rejection"
    );
}

#[then(regex = r"^the confirmation shall succeed$")]
async fn then_confirm_ok(w: &mut SeatWorld) {
    assert!(
        w.last_confirm.is_ok(),
        "expected confirm to succeed, got {:?}",
        w.last_confirm
    );
}

#[then(regex = r"^the confirmation shall fail$")]
async fn then_confirm_fail(w: &mut SeatWorld) {
    assert_eq!(
        w.last_confirm,
        Err(HoldError::NotHeld),
        "expected confirm to fail"
    );
}

#[then(regex = r"^the service shall report (\d+) seats available$")]
async fn then_available(w: &mut SeatWorld, expected: u32) {
    assert_eq!(w.map.available(w.now), expected);
}

// ---- Lifecycle FSM steps (REQ-002) ----

#[given(regex = r"^a hold in the (Held|Confirmed|Released|Expired) state$")]
async fn given_fsm_state(w: &mut SeatWorld, state: String) {
    w.fsm_state = parse_state(&state);
}

#[when(regex = r"^the (confirm|release|expire) event fires$")]
async fn when_fsm_event(w: &mut SeatWorld, event: String) {
    let ev = match event.as_str() {
        "confirm" => Event::Confirm,
        "release" => Event::Release,
        _ => Event::Expire,
    };
    w.fsm_after = next(w.fsm_state, ev);
}

#[then(regex = r"^the hold shall move to the (Confirmed|Released|Expired) state$")]
async fn then_fsm_moves(w: &mut SeatWorld, state: String) {
    assert_eq!(w.fsm_after, Some(parse_state(&state)));
}

#[then(regex = r"^the transition shall be rejected$")]
async fn then_fsm_rejected(w: &mut SeatWorld) {
    assert_eq!(w.fsm_after, None, "terminal state must reject the event");
}

fn parse_state(s: &str) -> State {
    match s {
        "Held" => State::Held,
        "Confirmed" => State::Confirmed,
        "Released" => State::Released,
        "Expired" => State::Expired,
        other => panic!("unknown state {other}"),
    }
}

fn main() {
    // NB: this crate is named `core`, which shadows std's `core` — so we cannot
    // use `#[tokio::main]` (its expansion references `core::future::…`). Build
    // the runtime explicitly. Specs are centralized at the workspace root.
    let rt = tokio::runtime::Runtime::new().expect("build tokio runtime");
    rt.block_on(SeatWorld::run("../../spec/features"));
}
