//! BDD suite — cucumber scenarios in `spec/features/*.feature`, the mandatory
//! EARS floor. Every REQ has at least one scenario here; each drives the pure
//! domain directly (no HTTP needed — the core is IO-free). The `.feature` files
//! live in the centralized `spec/features/`.
//!
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

use core::domain::ledger::{Ledger, LifecycleError, TransferError, TransferOutcome};
use core::domain::lifecycle::{Event, State};
use core::domain::parse::{parse_transfer_str, ParseError, TransferCommand};
use cucumber::{given, then, when, World as _};

/// One transfer attempt's result, kept so `Then` steps can assert on it.
type TransferResult = Result<TransferOutcome, TransferError>;

/// Scenario state: a ledger plus the most recent operation results.
#[derive(cucumber::World, Debug, Default)]
struct LedgerWorld {
    ledger: Ledger,
    // Parser scenarios:
    parse_input: String,
    parse_result: Option<Result<TransferCommand, ParseError>>,
    // Transfer scenarios — every attempt is pushed so multi-attempt scenarios
    // (idempotency, concurrency) can count applied vs rejected.
    transfer_results: Vec<TransferResult>,
    // Lifecycle scenarios:
    lifecycle_result: Option<Result<State, LifecycleError>>,
    // Query scenarios:
    queried_balance: Option<u64>,
    queried_state: Option<State>,
    queried: bool,
}

// ---- Given ---------------------------------------------------------------

#[given(regex = r"^an account (\d+) with balance (\d+)$")]
async fn given_account(w: &mut LedgerWorld, id: u64, balance: u64) {
    w.ledger.open_account(id, balance);
}

#[given(regex = r#"^the input line "(.*)"$"#)]
async fn given_input_line(w: &mut LedgerWorld, line: String) {
    w.parse_input = line;
}

// ---- When ----------------------------------------------------------------

#[when("the line is parsed")]
async fn when_parsed(w: &mut LedgerWorld) {
    w.parse_result = Some(parse_transfer_str(&w.parse_input));
}

#[when(regex = r#"^a transfer of (\d+) from (\d+) to (\d+) with key "(.*)" is attempted$"#)]
async fn when_transfer(w: &mut LedgerWorld, amount: u64, from: u64, to: u64, key: String) {
    let r = w.ledger.transfer(from, to, amount, &key);
    w.transfer_results.push(r);
}

#[when(regex = r"^account (\d+) is frozen$")]
async fn when_frozen(w: &mut LedgerWorld, id: u64) {
    w.lifecycle_result = Some(w.ledger.apply_lifecycle(id, Event::Freeze));
}

#[when(regex = r"^account (\d+) is unfrozen$")]
async fn when_unfrozen(w: &mut LedgerWorld, id: u64) {
    w.lifecycle_result = Some(w.ledger.apply_lifecycle(id, Event::Unfreeze));
}

#[when(regex = r"^account (\d+) is closed$")]
async fn when_closed(w: &mut LedgerWorld, id: u64) {
    w.lifecycle_result = Some(w.ledger.apply_lifecycle(id, Event::Close));
}

#[when(regex = r"^account (\d+) is queried$")]
async fn when_queried(w: &mut LedgerWorld, id: u64) {
    w.queried = true;
    w.queried_balance = w.ledger.balance(id);
    w.queried_state = w.ledger.state(id);
}

// ---- Then ----------------------------------------------------------------

#[then(regex = r#"^parsing shall succeed with from (\d+), to (\d+), amount (\d+) and key "(.*)"$"#)]
async fn then_parse_ok(w: &mut LedgerWorld, from: u64, to: u64, amount: u64, key: String) {
    let parsed = w
        .parse_result
        .clone()
        .expect("a parse was attempted")
        .expect("parse succeeded");
    assert_eq!(
        parsed,
        TransferCommand {
            from,
            to,
            amount_cents: amount,
            key,
        }
    );
}

#[then("parsing shall fail with a typed error")]
async fn then_parse_err(w: &mut LedgerWorld) {
    let r = w.parse_result.clone().expect("a parse was attempted");
    assert!(r.is_err(), "expected a typed parse error, got {r:?}");
}

#[then("the transfer shall be applied")]
async fn then_applied(w: &mut LedgerWorld) {
    let last = w.transfer_results.last().expect("a transfer was attempted");
    assert_eq!(last, &Ok(TransferOutcome::Applied), "expected Applied");
}

#[then("the transfer shall be rejected")]
async fn then_rejected(w: &mut LedgerWorld) {
    let last = w.transfer_results.last().expect("a transfer was attempted");
    assert!(last.is_err(), "expected a rejection, got {last:?}");
}

#[then("the transfer shall be a duplicate")]
async fn then_duplicate(w: &mut LedgerWorld) {
    let last = w.transfer_results.last().expect("a transfer was attempted");
    assert_eq!(last, &Ok(TransferOutcome::Duplicate), "expected Duplicate");
}

#[then("exactly one transfer shall be applied and the other rejected")]
async fn then_one_applied_one_rejected(w: &mut LedgerWorld) {
    let applied = w
        .transfer_results
        .iter()
        .filter(|r| matches!(r, Ok(TransferOutcome::Applied)))
        .count();
    let rejected = w.transfer_results.iter().filter(|r| r.is_err()).count();
    assert_eq!(applied, 1, "exactly one should apply");
    assert_eq!(rejected, 1, "exactly one should be rejected");
}

#[then(regex = r"^account (\d+) shall have balance (\d+)$")]
async fn then_balance(w: &mut LedgerWorld, id: u64, expected: u64) {
    assert_eq!(w.ledger.balance(id), Some(expected));
}

#[then(regex = r"^the total balance shall be (\d+)$")]
async fn then_total(w: &mut LedgerWorld, expected: u128) {
    assert_eq!(w.ledger.total_balance(), expected);
}

#[then(regex = r#"^account (\d+) shall be in state "(\w+)"$"#)]
async fn then_state(w: &mut LedgerWorld, id: u64, expected: String) {
    let state = w.ledger.state(id).expect("account exists");
    assert_eq!(state_name(state), expected);
}

#[then("the lifecycle transition shall be rejected")]
async fn then_lifecycle_rejected(w: &mut LedgerWorld) {
    let r = w
        .lifecycle_result
        .as_ref()
        .expect("a lifecycle op was attempted");
    assert!(r.is_err(), "expected an illegal-transition rejection");
}

#[then(regex = r#"^the query shall report balance (\d+) and state "(\w+)"$"#)]
async fn then_query_reports(w: &mut LedgerWorld, balance: u64, state: String) {
    assert!(w.queried, "a query was attempted");
    assert_eq!(w.queried_balance, Some(balance));
    assert_eq!(w.queried_state.map(state_name), Some(state));
}

#[then("the query shall report the account is unknown")]
async fn then_query_unknown(w: &mut LedgerWorld) {
    assert!(w.queried, "a query was attempted");
    assert_eq!(w.queried_balance, None);
    assert_eq!(w.queried_state, None);
}

fn state_name(s: State) -> String {
    match s {
        State::Open => "Open",
        State::Frozen => "Frozen",
        State::Closed => "Closed",
    }
    .to_owned()
}

fn main() {
    // NB: this crate is named `core`, which shadows std's `core` — so we cannot
    // use `#[tokio::main]` (its expansion references `core::future::…` and would
    // resolve here). Build the runtime explicitly instead.
    //
    // Path is relative to this crate (crates/core); specs are centralized at the
    // workspace root in spec/features/.
    let rt = tokio::runtime::Runtime::new().expect("build tokio runtime");
    rt.block_on(LedgerWorld::run("../../spec/features"));
}
