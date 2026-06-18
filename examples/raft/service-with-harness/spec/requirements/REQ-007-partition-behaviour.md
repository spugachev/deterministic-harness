---
id: REQ-007
title: The cluster survives a network partition without losing committed writes
status: active
acceptance:
  - When a partition isolates a minority the system shall keep serving writes on the majority side
  - When a partition isolates a minority the system shall reject or hold writes on the minority side
  - When the partition heals the system shall let the minority catch up with no committed write lost or duplicated (verified=dst)
implements_in:
  gherkin: [spec/features/partition.feature]
  code:
    - crates/core/src/domain/raft/node/replicate.rs::client_propose
    - crates/core/src/domain/raft/decide.rs::is_majority
  dst: [crates/sim/tests/dst.rs]
---

## Rationale

This is the acceptance bar. The majority side retains a leader and keeps
committing because it can still reach a quorum; the isolated minority can never
gather a majority, so `client_propose` on a non-leader is rejected and any node
that believes it leads can never advance its commit index. On heal, AppendEntries
back-fills the minority from the leader's log, so the committed history is
preserved exactly — no write lost, duplicated, or reordered. The `sim` DST
harness injects the partition, drives writes, heals, and asserts a linearizable
committed history across all nodes.
