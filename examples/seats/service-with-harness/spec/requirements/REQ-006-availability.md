---
id: REQ-006
title: The service reports current availability
status: active
acceptance:
  - When a client queries availability the system shall report capacity minus confirmed and currently-held seats
implements_in:
  gherkin: [spec/features/availability.feature]
  code: [crates/core/src/domain/seats.rs::available]
  proptest: [crates/core/src/domain/seats.rs]
---

## Rationale

Clients need to know how many seats they can still hold. Availability is the
exact complement of occupancy: `capacity - (confirmed + live_held(now))`,
saturating at zero so it never underflows. It reflects lazy expiry — seats from
a hold past its TTL are reported as available. Over HTTP this maps to a
`200 OK` with the available count in the body.
