---
id: REQ-001
title: Hold seats with a unique id and a TTL
status: active
acceptance:
  - When a client requests N seats and at least N seats are available the system shall grant a hold with a unique id and a TTL
  - When a client requests N seats and fewer than N seats are available the system shall reject the request for insufficient availability
  - When a client requests zero seats the system shall reject the request
implements_in:
  gherkin: [spec/features/seats.feature]
  code:
    - crates/core/src/domain/reservation.rs::hold
    - crates/core/src/domain/hold.rs::HoldState
---

## Rationale

A hold reserves seats for a client without committing them. It is granted only
when enough seats are available (counting neither expired holds nor confirmed
seats as available), receives a unique id from the `IdGen` port, and expires
after a fixed TTL measured against the `Clock` port. This is the entry point to
the lifecycle modelled in `spec/tla/Lifecycle.tla` (the `Held` initial state).
