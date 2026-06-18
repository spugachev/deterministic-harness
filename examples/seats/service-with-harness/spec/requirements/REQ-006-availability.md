---
id: REQ-006
title: Report currently available seats
status: active
acceptance:
  - When a client queries availability the system shall report capacity minus confirmed minus currently-held seats
  - While holds have expired the system shall count their seats as available again
implements_in:
  gherkin: [spec/features/seats.feature]
  code:
    - crates/core/src/domain/reservation.rs::available
---

## Rationale

Availability is the derived quantity clients use to decide whether to attempt a
hold: `capacity - confirmed - active_held`, computed against the current time so
expired holds do not count against it. It never underflows (proven by the
`available_never_underflows` Kani harness).
