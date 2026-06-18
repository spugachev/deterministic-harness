---
id: REQ-006
title: Concurrent transfers never double-spend or violate conservation
status: active
acceptance:
  - While multiple transfers race on the same accounts the service shall never drive a balance negative and shall never double-spend
  - While multiple transfers race the sum of all balances shall remain invariant (verified=dst)
implements_in:
  gherkin: [spec/features/concurrency.feature]
  code: [crates/api/src/state.rs::SharedLedger]
  dst: [crates/api/tests/dst.rs]
---

## Rationale

The service must be safe under concurrent transfers racing on the same accounts.
The ledger aggregate is pure and single-threaded; the outer `api` crate wraps it
behind a lock so each transfer is applied atomically. The DST drives many
interleaved transfers over a seeded schedule and asserts no overdraft and total
conservation, replaying deterministically on failure.
