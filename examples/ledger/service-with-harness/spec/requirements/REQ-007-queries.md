---
id: REQ-007
title: Query an account's balance and lifecycle state
status: active
acceptance:
  - When an existing account is queried the service shall report its current balance and lifecycle state
  - When a non-existent account is queried the service shall report that it is unknown rather than fabricating a balance
implements_in:
  gherkin: [spec/features/query.feature]
  code: [crates/core/src/domain/ledger.rs::balance, crates/core/src/domain/ledger.rs::state]
  proptest: [crates/core/src/domain/ledger.rs]
---

## Rationale

Callers can observe an account's balance and lifecycle state. A query for an
unknown account returns "no such account" (an `Option::None`) rather than a
default zero balance, so absence is distinguishable from an empty account.
