---
id: REQ-001
title: Hold grants seats when available, rejects when not
status: active
acceptance:
  - When a client requests N seats and at least N seats are available the service shall grant a hold with a unique id and a TTL
  - When a client requests N seats and fewer than N are available the service shall reject the hold for insufficient availability (verified=kani)
  - When a client requests zero seats the service shall reject the hold
implements_in:
  gherkin: [spec/features/holds.feature]
  code:
    - crates/core/src/domain/seats.rs::hold
    - crates/core/src/domain/seats.rs::grant_step
  kani: [crates/core/src/domain/proofs.rs]
  proptest: [crates/core/src/domain/seats.rs]
---

## Rationale

A client reserves seats with a `hold`, which carries a unique id (from the
`IdGen` port) and an expiry derived from the current time (`Clock` port) plus a
fixed TTL. A hold succeeds only when the requested count fits in current
availability — the no-overbooking step proven exhaustively by Kani
(`grant_step`). Zero-seat requests are rejected so every hold reserves at least
one seat. The HTTP layer maps the rejection to `409 Conflict`; the domain
expresses it as `SeatError::InsufficientAvailability`.
