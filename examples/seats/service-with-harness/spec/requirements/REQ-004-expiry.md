---
id: REQ-004
title: Holds expire automatically after their TTL
status: active
acceptance:
  - While a hold has not been confirmed within its TTL the system shall free its seats automatically based on the current time
  - When an operation observes that a hold has expired the system shall not count its seats as held or confirmed
implements_in:
  gherkin: [spec/features/seats.feature]
  code:
    - crates/core/src/domain/reservation.rs::reclaim_expired
  tla: [spec/tla/Lifecycle.tla::Expire]
---

## Rationale

A hold not confirmed within its TTL expires (the FSM's `Expire` event,
transitioning `Held` to `Expired`). Expiry is lazy: any operation that reads the
`Clock` port reclaims holds whose `expires_at` has passed, so their seats become
available again without a background sweeper. A hold is valid while
`now <= expires_at` and expired after.
