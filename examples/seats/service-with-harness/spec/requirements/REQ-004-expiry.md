---
id: REQ-004
title: Holds expire automatically after their TTL
status: active
acceptance:
  - While a hold has not been confirmed within its TTL the service shall automatically free its seats based on the current time (verified=proptest)
  - When the current time reaches a hold's expiry instant the service shall no longer count its seats as held
implements_in:
  gherkin: [spec/features/holds.feature]
  code:
    - crates/core/src/domain/seats.rs::live_held
    - crates/core/src/domain/hold.rs::next
  tla: [spec/tla/Lifecycle.tla]
---

## Rationale

Expiry is lazy and time-driven: a hold is live at instant `now` iff
`now < expires_at`, where `now` comes from the `Clock` port. The moment any
operation observes a time at or past `expires_at`, the hold's seats stop
counting against availability — no background sweeper is needed, and expiry is
deterministically testable by advancing a `FixedClock`. The FSM models this as
`Held → Expired`.
