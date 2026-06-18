---
id: REQ-006
title: Availability reports the free seat count
status: active
acceptance:
  - When a client queries availability the service shall report capacity minus confirmed minus currently-held seats (verified=proptest)
implements_in:
  gherkin: [spec/features/availability.feature]
  code: [crates/core/src/domain/seats.rs::available]
  proptest: [crates/core/src/domain/seats.rs]
---

## Rationale

Availability is the accounting complement of what is in use:
`capacity - confirmed - live-held`, computed with saturating arithmetic so it
never underflows. It reflects lazy expiry — expired holds free their seats — and
is exposed read-only over HTTP (`GET`, `200 OK`).
