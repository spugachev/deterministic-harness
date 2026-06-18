---
id: REQ-002
title: KV state machine applies committed commands deterministically
status: active
acceptance:
  - When a committed SET is applied the system shall store the value at the key
  - When a committed GET is applied the system shall report the current value or report no value if absent
  - When a committed DEL is applied the system shall remove the key
  - Replaying the same committed command sequence shall always yield identical state (verified=proptest)
implements_in:
  gherkin: [spec/features/kv_state_machine.feature]
  code: [crates/core/src/domain/kv.rs::Kv]
  proptest: [crates/core/src/domain/kv.rs]
---

## Rationale

Once Raft commits a command, every node feeds it to this state machine. Apply is
a pure function of `(current state, committed command)`, so the same committed
log replayed on any node yields the same `BTreeMap` — the local face of State
Machine Safety. A proptest asserts that replaying an arbitrary command sequence
on two fresh stores produces equal state.
