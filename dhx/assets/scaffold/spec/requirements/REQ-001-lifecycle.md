---
id: REQ-001
title: Todo lifecycle FSM
status: active
acceptance:
  - The transition function shall be total and panic-free for every state/event pair (verified=proptest)
  - Archived shall be terminal — no event leaves it (verified=tla)
implements_in:
  code: [crates/core/src/domain/state.rs::next]
  tla:  [spec/tla/Lifecycle.tla]
---

## Rationale

The lifecycle is a finite state machine: `Active ⇄ Done`, and either may be
`Archive`d into the terminal `Archived` state. `state::next` is the single
source of truth — pure, total, panic-free — which `dhx regen` projects into
`spec/tla/Lifecycle.{tla,cfg}` so the model checker verifies the same table the
runtime executes. Totality is a Kani target; terminality is a TLA+ invariant.
