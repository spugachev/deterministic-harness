---
id: REQ-002
title: Confirm books a live hold, rejects otherwise
status: active
acceptance:
  - When a client confirms a hold that is live the service shall permanently book its seats
  - When a client confirms a hold that is expired unknown or already confirmed the service shall reject the confirmation (verified=proptest)
implements_in:
  gherkin: [spec/features/holds.feature]
  code:
    - crates/core/src/domain/seats.rs::confirm
    - crates/core/src/domain/hold.rs::next
  tla: [spec/tla/Lifecycle.tla]
---

## Rationale

Confirming a live hold moves it `Held → Confirmed` (the lifecycle FSM) and turns
its held seats into permanent bookings in the ledger. A hold that has expired,
never existed, or was already confirmed/released is not live, so confirmation is
rejected (`SeatError::UnknownHold`; HTTP `404 Not Found`). The FSM makes
"terminal states reject every event" a structural fact checked by TLC.
