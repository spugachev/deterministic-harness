//! BDD suite — cucumber scenarios in `spec/features/*.feature`, the mandatory
//! EARS floor. Every REQ has at least one scenario here; each drives the pure
//! domain directly (no HTTP needed — the core is IO-free). The `.feature` files
//! live in the centralized `spec/features/` next to the other specs.
//!
//! This wires up the throwaway REQ-001 example; replace its steps as you replace
//! the example domain. Run with `cargo test -p core --test bdd`.
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

use core::domain::example::grant;
use cucumber::{given, then, when, World as _};

/// Scenario state: the remaining budget and the last grant result.
#[derive(cucumber::World, Debug, Default)]
struct ExampleWorld {
    remaining: u32,
    granted: Option<u32>,
}

#[given(regex = r"^a remaining budget of (\d+)$")]
async fn given_budget(w: &mut ExampleWorld, remaining: u32) {
    w.remaining = remaining;
}

#[when(regex = r"^a request for (\d+) is made$")]
async fn when_request(w: &mut ExampleWorld, requested: u32) {
    w.granted = Some(grant(requested, w.remaining));
}

#[then(regex = r"^the system shall grant (\d+)$")]
async fn then_granted(w: &mut ExampleWorld, expected: u32) {
    assert_eq!(
        w.granted,
        Some(expected),
        "expected to grant {expected}, got {:?}",
        w.granted
    );
}

fn main() {
    // NB: this crate is named `core`, which shadows std's `core` — so we cannot
    // use `#[tokio::main]` (its expansion references `core::future::…` and would
    // resolve here). Build the runtime explicitly instead.
    //
    // Path is relative to this crate (crates/core); specs are centralized at the
    // workspace root in spec/features/.
    let rt = tokio::runtime::Runtime::new().expect("build tokio runtime");
    rt.block_on(ExampleWorld::run("../../spec/features"));
}
