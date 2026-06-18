---
id: REQ-003
title: Confirming a hold books its seats permanently
status: active
acceptance:
  - When a client confirms a live hold by id the system shall book its seats permanently
  - When a client confirms an expired, unknown, or already-confirmed hold the system shall reject the confirmation
implements_in:
  gherkin: [spec/features/confirm.feature]
  code: [crates/core/src/domain/seats.rs::confirm]
  proptest: [crates/core/src/domain/seats.rs]
---

## Rationale

Confirmation is the commit step: a live hold's seats move from the transient
held pool to the permanent confirmed count, and the hold id is consumed.
Confirming a hold that is expired, never existed, or was already
confirmed/released must fail — there is nothing valid to commit. Over HTTP a
successful confirm maps to `200 OK` and a failure to `409 Conflict`.
