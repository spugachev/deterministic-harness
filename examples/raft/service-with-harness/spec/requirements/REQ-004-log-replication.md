---
id: REQ-004
title: Log replication commits entries on a majority and applies them in order
status: active
acceptance:
  - When a client command reaches the leader the system shall append it to the leader log and replicate via AppendEntries
  - When an entry from the leader's current term is stored by a majority the system shall commit it
  - When an entry is committed the system shall apply it to the KV state machine in log order on every node
  - The commit index shall only advance and never regress (verified=kani)
implements_in:
  gherkin: [spec/features/replication.feature]
  code:
    - crates/core/src/domain/raft/node/replicate.rs::client_propose
    - crates/core/src/domain/raft/decide.rs::new_commit_index
    - crates/core/src/domain/raft/log.rs::Log
  kani: [crates/core/src/domain/raft/decide.rs]
  dst: [crates/sim/tests/dst.rs]
---

## Rationale

The leader appends client commands and replicates them with AppendEntries; an
entry is committed once a majority stores it AND it was created in the leader's
current term (`decide::new_commit_index`). Committed entries apply to the KV
state machine in log order. Kani proves the commit index is monotonic and only
advances on a current-term majority; the DST cluster exercises the whole
replicate-commit-apply path end to end.
