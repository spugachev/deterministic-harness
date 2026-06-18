---
id: REQ-002
title: Transfer moves money only when valid, with no overdraft
status: active
acceptance:
  - When the source has sufficient funds and both accounts are open the service shall move the amount from source to destination
  - When the amount is zero, the source equals the destination, the source has insufficient funds, or either account is frozen or closed the service shall reject the transfer with a typed error and change no state
  - The source balance shall never become negative as a result of a transfer (verified=kani)
implements_in:
  gherkin: [spec/features/transfer.feature]
  code: [crates/core/src/domain/money.rs::apply_transfer, crates/core/src/domain/ledger.rs::transfer]
  kani: [crates/core/src/domain/money.rs]
  proptest: [crates/core/src/domain/money.rs]
---

## Rationale

A transfer either moves the exact amount between two open accounts or is a typed
no-op. No-overdraft (balances are unsigned and `from < amount` is rejected) is
the safety property; it is proved exhaustively by Kani on the scalar money step
and sampled by proptest, and enforced end-to-end by the ledger aggregate.
