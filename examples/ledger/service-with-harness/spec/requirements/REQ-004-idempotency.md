---
id: REQ-004
title: Transfers are idempotent by key — money moves at most once
status: active
acceptance:
  - When a transfer with a new idempotency key is submitted the service shall apply it and record the key
  - When a transfer with an already-applied idempotency key is re-submitted the service shall return the original outcome and move no money (verified=proptest)
implements_in:
  gherkin: [spec/features/idempotency.feature]
  code: [crates/core/src/domain/ledger.rs::transfer]
  proptest: [crates/core/src/domain/ledger.rs]
  dst: [crates/api/tests/dst.rs]
---

## Rationale

Each transfer carries an opaque idempotency key. The first application records
the key and moves the money; any replay of the same key is a no-op returning the
recorded `Duplicate` outcome. This makes the protocol safe to retry over an
unreliable channel — exactly-once money movement under at-least-once delivery.
