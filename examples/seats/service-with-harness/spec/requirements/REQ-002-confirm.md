---
id: REQ-002
title: Confirm a hold before it expires
status: active
acceptance:
  - When a client confirms a live hold by id the system shall permanently book its seats
  - When a client confirms an expired, unknown, released, or already-confirmed hold the system shall reject the confirmation
implements_in:
  gherkin: [spec/features/seats.feature]
  code:
    - crates/core/src/domain/reservation.rs::confirm
  tla: [spec/tla/Lifecycle.tla::Confirm]
---

## Rationale

Confirming a hold transitions it from `Held` to `Confirmed` (the FSM's `Confirm`
event), moving its seats from the held set to the permanently-booked total. Only
a live `Held` hold can be confirmed; every terminal state (expired, released,
already-confirmed) has no `Confirm` transition, so the confirmation is rejected.
