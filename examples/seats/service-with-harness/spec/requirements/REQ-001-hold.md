---
id: REQ-001
title: Holds never oversell the venue
status: active
acceptance:
  - When at least N seats are free the service shall grant a hold for N seats with a unique id and a TTL
  - When fewer than N seats are free the service shall reject the hold for N seats
  - The confirmed plus currently-held seats shall never exceed the venue capacity (verified=kani)
implements_in:
  gherkin: [spec/features/holds.feature]
  code: [crates/core/src/domain/seats.rs::hold]
  kani: [crates/core/src/domain/proofs.rs]
  proptest: [crates/core/src/domain/seats.rs]
---

## Rationale

The core safety property of a ticketing service: it must never sell the same
seat twice. A hold reserves seats only when capacity allows; otherwise it is
rejected. The invariant `confirmed + live_held(now) <= capacity` is the heart
of the domain. It is proven exhaustively over scalar inputs by the Kani proof
on the pure `grant` step, and over long operation sequences by the `proptest`
in `seats.rs`. Over HTTP, a granted hold maps to `201 Created` and a rejection
to `409 Conflict`.
