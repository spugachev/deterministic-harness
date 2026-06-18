---
id: ADR-0002
title: Concurrency safety by serializing operations behind one lock
status: accepted
implements: [REQ-007]
---

## Decision

The verified core (`SeatMap`) is single-threaded and pure. The concurrent
service achieves safety by holding the `SeatMap` behind a single `Mutex` and
minting hold ids from one `IdGen` under the same lock (`api::state::AppState`).
Every hold/confirm/release/availability operation is therefore fully
serialized.

## Consequence

"No overbooking under concurrency" reduces to "no overbooking over any *serial*
interleaving of operations" — exactly the property proptest and Kani prove on
the pure core. Two clients racing for the last seat cannot both win, because
the availability check and the hold insertion happen atomically under the lock.
The DST harness drives randomized serialized sequences under a seeded
deterministic clock and asserts the invariant after every step. The trade-off
is throughput (one operation at a time), acceptable for a single-event ledger;
sharding by event would lift it later without changing the core.
