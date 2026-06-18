---
id: REQ-003
title: The sum of all balances is invariant under every operation
status: active
acceptance:
  - When a transfer is applied the sum of all account balances shall be unchanged (verified=kani)
  - When a transfer is rejected the sum of all account balances shall be unchanged (verified=proptest)
implements_in:
  gherkin: [spec/features/conservation.feature]
  code: [crates/core/src/domain/money.rs::apply_transfer, crates/core/src/domain/ledger.rs::transfer]
  kani: [crates/core/src/domain/money.rs]
  proptest: [crates/core/src/domain/ledger.rs]
  dst: [crates/api/tests/dst.rs]
---

## Rationale

Money is conserved: a transfer moves cents between accounts, never creating or
destroying them, and a rejected transfer changes nothing. The scalar step proves
`new_from + new_to == from + to` under Kani; a proptest over random operation
sequences asserts `total_balance()` is invariant; and the DST replays concurrent
sequences and checks the same total.
