---
id: REQ-001
title: Seat lifecycle FSM
status: active
acceptance:
  - The transition function shall be total and panic-free for every state/event pair (verified=proptest)
  - Cancelled shall be terminal — no event leaves it (verified=tla)
implements_in:
  code: [crates/core/src/domain/state.rs::next]
  tla:  [spec/tla/Lifecycle.tla]
---

## Rationale

A seat is a finite state machine: `Free → Held → Confirmed`, with `Release` and
`Expire` returning a held/confirmed seat to `Free`, and `Cancel` driving a
held/confirmed seat to the terminal `Cancelled`. `state::next` is the single
source of truth — pure, total, panic-free — which `dhx regen` projects into
`spec/tla/Lifecycle.{tla,cfg}` so the model checker verifies the same table the
runtime executes. Totality is proven by proptest; terminality of `Cancelled` is
a TLA+ invariant (`ArchivedTerminal`), kept non-vacuous by `mutations.toml`.
