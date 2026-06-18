---
id: REQ-005
title: Account lifecycle is Open to Frozen to Closed, Closed terminal
status: active
acceptance:
  - When an open account is frozen, unfrozen, or closed, or a frozen account is unfrozen or closed, the service shall apply the transition
  - When a transition is not legal from the current state, including any transition on a closed account, the service shall reject it and change no state (verified=tla)
implements_in:
  gherkin: [spec/features/lifecycle.feature]
  code: [crates/core/src/domain/lifecycle.rs::next, crates/core/src/domain/ledger.rs::apply_lifecycle]
  tla: [spec/tla/Lifecycle.tla]
  proptest: [crates/core/src/domain/lifecycle.rs]
---

## Rationale

An account moves `Open → Frozen → Closed`; `Closed` is terminal. Transitions are
explicit operations modelled as a pure `fn next(state, event) -> Option<state>`,
from which `dhx regen` generates the TLA+ spec. Only legal transitions return
`Some`; everything else (including any event on a closed account) is rejected
with no state change.
