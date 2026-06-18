---
id: REQ-003
title: Release an unconfirmed hold
status: active
acceptance:
  - When a client releases a live unconfirmed hold by id the system shall return its seats to available
  - When a client releases an expired or unknown hold the system shall treat it as a no-op without error
implements_in:
  gherkin: [spec/features/seats.feature]
  code:
    - crates/core/src/domain/reservation.rs::release
  tla: [spec/tla/Lifecycle.tla::Release]
---

## Rationale

Releasing a `Held` hold transitions it to `Released` (the FSM's `Release` event)
and returns its seats to the available pool. Release is idempotent: releasing an
unknown or already-expired hold does nothing and reports success, so a retried
release (e.g. a client retry after a network blip) is always safe.
