---
id: REQ-002
title: A hold follows a one-way lifecycle
status: active
acceptance:
  - While a hold is Held the system shall allow exactly one of confirm, release, or expire
  - When a hold has reached a terminal state the system shall reject every further lifecycle event (verified=tla)
implements_in:
  gherkin: [spec/features/lifecycle.feature]
  code: [crates/core/src/domain/hold.rs::next]
  tla: [spec/tla/Lifecycle.tla]
---

## Rationale

Each hold is a tiny state machine: born `Held`, it makes exactly one terminal
transition — `Confirmed`, `Released`, or `Expired` — and accepts no event
thereafter. Modelling it as a pure `fn next(state, event) -> Option<state>`
lets `dhx regen` project the transition relation to `spec/tla/Lifecycle.tla`,
so TLC model-checks the same relation the Rust executes. The one-way property
(no transition ever returns to `Held`, terminal states are sinks) is what
guarantees a confirmed booking cannot be silently released or re-confirmed.
