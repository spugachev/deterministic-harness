---
id: REQ-007
title: Concurrent holds never both win the last seats
status: active
acceptance:
  - When multiple clients race for the last seats the system shall grant at most the seats that remain (verified=dst)
  - The service shall serialize every operation so the no-overbooking invariant holds under concurrency
implements_in:
  gherkin: [spec/features/concurrency.feature]
  code: [crates/api/src/state.rs::AppState]
  dst: [crates/api/tests/dst.rs]
---

## Rationale

Two clients racing for the last seat must not both succeed. The verified core
is single-threaded and pure; the outer service achieves concurrency safety by
serializing every operation behind a single lock over the `SeatMap`
(ADR-0002). "No overbooking under concurrency" therefore reduces to "no
overbooking over any *serial* interleaving" — exactly the property proptest and
Kani already prove on the core. The DST harness (`crates/api/tests/dst.rs`)
drives randomized concurrent-style operation sequences against the real locked
state under a seeded deterministic clock, asserting the invariant after every
step; a failure replays deterministically from its seed.
