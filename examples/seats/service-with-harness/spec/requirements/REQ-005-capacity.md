---
id: REQ-005
title: Confirmed plus held seats never exceed capacity
status: active
acceptance:
  - While any sequence of hold, confirm, release, and expiry operations is applied the system shall keep confirmed plus currently-held seats at most the event capacity (verified=kani, proptest)
  - When two clients race for the last seats the system shall grant at most the available seats so no overbooking occurs (verified=kani, proptest)
implements_in:
  gherkin: [spec/features/seats.feature]
  code:
    - crates/core/src/domain/reservation.rs::available
  kani: [crates/core/src/domain/proofs.rs::no_overbooking_under_operations]
---

## Rationale

The safety property of the whole service: there is no sequence of operations,
and no concurrent interleaving, under which `confirmed + currently_held` exceeds
`capacity`. It holds by construction — the only seat-reserving operation,
`hold`, first reclaims expired holds and then refuses unless enough seats remain
available. This is not externally observable as a single HTTP status, so it is
proven by the Kani harness (`no_overbooking_under_operations`) and a proptest
property rather than asserted by a scenario alone, but a BDD scenario still
exercises the racing-for-the-last-seats case.
