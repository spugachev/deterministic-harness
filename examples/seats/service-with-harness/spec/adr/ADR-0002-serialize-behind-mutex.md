---
id: ADR-0002
title: The application serializes ledger operations behind one mutex
status: accepted
implements: [REQ-005]
---

## Decision

The verified core (`SeatMap`) is single-threaded and IO-free. The outer
application wraps it in a single `Mutex<SeatMap>` and takes the lock for the
whole of every hold/confirm/release/availability operation. Concurrent HTTP
requests therefore see a serial order of ledger operations.

## Consequence

"No overbooking under concurrent requests" (REQ-005) reduces to a property of
*serial* operation sequences — exactly what Kani proves for the per-grant step
and proptest exercises over long sequences. The DST harness drives concurrent
holds through the locked ledger with a seed to confirm two clients racing for
the last seat never both succeed. There is no lock-free fast path in the core,
so there is no in-core data race to model with Loom.
