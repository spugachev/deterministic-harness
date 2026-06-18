---
id: ADR-0001
title: Non-determinism flows through ports
status: accepted
implements: [REQ-001, REQ-004]
---

## Decision

All wall-clock, randomness, and id generation go through the `ports` traits
(Clock/Rng/IdGen). Production wires real adapters; tests/DST wire seeded
deterministic ones. `clippy.toml` bans the direct calls.

## Consequence

The domain is reproducible and the DST/Loom/TSAN gates are meaningful — a
schedule or seed fully determines a run.
