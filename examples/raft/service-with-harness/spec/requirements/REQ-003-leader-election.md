---
id: REQ-003
title: Leader election produces a single leader per term
status: active
acceptance:
  - When a follower's election timeout elapses the system shall make it a candidate that increments its term and requests votes
  - When a candidate collects votes from a majority the system shall make it the leader
  - When votes split the system shall resolve the election via randomized timeouts (verified=dst)
  - A node shall grant at most one vote per term and only to a candidate whose log is at least as up-to-date (verified=kani)
implements_in:
  gherkin: [spec/features/election.feature]
  code:
    - crates/core/src/domain/raft/role.rs::next
    - crates/core/src/domain/raft/decide.rs::should_grant_vote
  kani: [crates/core/src/domain/raft/decide.rs]
  dst: [crates/sim/tests/dst.rs]
---

## Rationale

Election is the Follower → Candidate → Leader role machine (`role::next`, the
`[fsm]` source lifted into TLA+) driven by the vote-granting rule
(`decide::should_grant_vote`). The single-vote-per-term and up-to-date-log
preconditions of Election Safety are proven exhaustively by Kani; split-vote
resolution via randomized (Rng-port) timeouts is exercised by the seeded DST
cluster.
