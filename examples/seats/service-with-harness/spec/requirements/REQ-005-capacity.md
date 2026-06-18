---
id: REQ-005
title: The capacity invariant is never violated
status: active
acceptance:
  - While the service runs the sum of confirmed and currently-held seats shall never exceed the event capacity (verified=kani)
  - When two clients race for the last seats the service shall not grant both holds (verified=dst)
implements_in:
  gherkin: [spec/features/capacity.feature]
  code:
    - crates/core/src/domain/seats.rs::grant_step
    - crates/core/src/domain/seats.rs::hold
  kani: [crates/core/src/domain/proofs.rs]
  proptest: [crates/core/src/domain/seats.rs]
  dst: [crates/api/tests/dst.rs]
---

## Rationale

The core safety property: `confirmed + live-held <= capacity` under ANY sequence
of operations or concurrent requests — no overbooking, ever. Kani proves the
per-grant step preserves it exhaustively (`grant_step`); proptest exercises it
across long op sequences on a monotonic clock; the DST harness drives concurrent
holds over a simulated world with a seed. Concurrency safety holds because the
application serializes every operation behind a single `Mutex<SeatMap>`
(ADR-0002), reducing "no overbooking under races" to the serial property the
core proves.
