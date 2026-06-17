---
id: REQ-003
title: Hold expiry via the Clock port
status: active
acceptance:
  - A hold shall be considered expired exactly when the clock time is at or after its deadline (verified=proptest)
  - Once expired a hold shall remain expired at every later time (verified=proptest)
implements_in:
  code: [crates/core/src/domain/hold.rs::is_expired]
---

## Rationale

A `Held` seat times out. `hold::is_expired(clock, hold)` reads "now" through the
[`Clock`](crate::ports::Clock) port — never `SystemTime::now` (banned by
`clippy.toml`) — so the decision is deterministic and reproducible under DST.
Expiry is inclusive of the boundary (`now >= expires_at`). Two proptest laws pin
the behaviour: it equals the boundary comparison for every input, and it is
monotonic in time (expiry never flips back to live).
