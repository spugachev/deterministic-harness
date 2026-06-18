---
id: ADR-0001
title: Non-determinism flows through ports
status: accepted
implements: [REQ-001, REQ-004, REQ-005]
---

## Decision

All wall-clock, randomness, and id generation go through the `ports` traits
(Clock/Rng/IdGen). The reservation domain reads time only via `Clock` (for TTL
expiry, REQ-004) and mints hold ids only via `IdGen` (REQ-001). Production wires
real adapters; tests/DST wire seeded deterministic ones. `clippy.toml` bans the
direct calls (`SystemTime::now`, `Instant::now`, thread RNG).

## Consequence

The domain is reproducible and the no-overbooking proof (REQ-005) is meaningful:
a seed plus a sequence of operations fully determines a run, so the Kani/proptest
capacity invariant and any DST replay are deterministic.
