---
id: REQ-005
title: Raft safety invariants hold under any message interleaving
status: active
acceptance:
  - The system shall maintain at most one leader per term (verified=kani)
  - The system shall keep logs identical up to any index where two logs share an entry index and term (verified=dst)
  - The system shall ensure no two nodes apply different commands at the same log index (verified=dst)
  - The system shall prevent a minority partition from committing any write (verified=dst)
implements_in:
  gherkin: [spec/features/safety.feature]
  code:
    - crates/core/src/domain/raft/decide.rs::candidate_log_is_up_to_date
    - crates/core/src/domain/raft/log.rs::Log
  tla: [spec/tla/Lifecycle.tla]
  kani: [crates/core/src/domain/raft/decide.rs]
  dst: [crates/sim/tests/dst.rs]
---

## Rationale

These are the point of the whole exercise — they must hold under any
interleaving, loss, reorder, and partition. The role machine's reachable states
are model-checked in TLA+ (`spec/tla/Lifecycle.tla`, generated from
`role::next`). The scalar safety arithmetic (one vote per term, the up-to-date
comparison, the strict-majority overlap that rules out two disjoint majorities)
is proven exhaustively by Kani. Log Matching, State Machine Safety, and the
no-split-brain property — statements about the whole cluster — are checked by the
seeded DST harness after every run, including under an injected partition.
