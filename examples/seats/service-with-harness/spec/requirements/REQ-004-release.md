---
id: REQ-004
title: Releasing a hold returns its seats and is idempotent
status: active
acceptance:
  - When a client releases an unconfirmed hold by id the system shall return its seats to available
  - When a client releases an unknown or already-expired hold the system shall treat it as a no-op (verified=proptest)
implements_in:
  gherkin: [spec/features/release.feature]
  code: [crates/core/src/domain/seats.rs::release]
  proptest: [crates/core/src/domain/seats.rs]
---

## Rationale

Release is the voluntary cancel: an unconfirmed hold's seats return to the
available pool immediately. Because clients retry and holds may have already
expired, release is idempotent — releasing an unknown or expired id changes
nothing and always succeeds. Over HTTP release maps to `204 No Content`
regardless of whether the id was live, so retries are safe.
