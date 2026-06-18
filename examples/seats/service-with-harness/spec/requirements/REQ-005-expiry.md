---
id: REQ-005
title: Unconfirmed holds expire and free their seats
status: active
acceptance:
  - When a hold is not confirmed within its TTL the system shall free its seats automatically
  - While a hold is within its TTL the system shall keep its seats reserved (verified=proptest)
implements_in:
  gherkin: [spec/features/expiry.feature]
  code: [crates/core/src/domain/seats.rs::live_held]
  proptest: [crates/core/src/domain/seats.rs]
---

## Rationale

A hold must not pin seats forever. Expiry is evaluated lazily against the
current clock: a hold counts toward occupancy only while `now < expires_at`,
so the next operation after the TTL elapses sees its seats as free without any
background sweeper. This keeps the domain pure and deterministic — the clock
(injected through the `Clock` port) fully determines which holds are live. The
invariant `confirmed + live_held(now) <= capacity` therefore holds only under a
monotonically non-decreasing clock (ADR-0001).
