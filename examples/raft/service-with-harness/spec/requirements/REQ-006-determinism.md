---
id: REQ-006
title: Cluster runs are deterministic and seed-reproducible
status: active
acceptance:
  - All wall-clock time shall flow through the Clock port and all randomness through the Rng and IdGen ports
  - When run twice from the same seed the system shall produce the same outcome (verified=dst)
implements_in:
  gherkin: [spec/features/determinism.feature]
  code:
    - crates/core/src/domain/raft/node.rs::Node
    - crates/core/src/ports/mod.rs
  dst: [crates/sim/tests/dst.rs]
---

## Rationale

Determinism is the precondition that makes the DST gate meaningful. The node
takes time from a [`Clock`] and timeout jitter / ids from [`Rng`]/[`IdGen`];
`clippy.toml` bans the ambient `SystemTime::now`/`Instant::now`/thread-RNG calls.
A whole cluster run — elections, replication, partition, heal — is therefore a
pure function of the seed, so any DST failure replays identically. The BDD
scenario re-runs an election from the same seed and asserts the same leader; the
DST suite is seeded throughout.
