//! Account lifecycle as a pure state machine: `Open → Frozen → Closed`.
//!
//! Modelled as a pure transition function (`next` below, mapping a state and an
//! event to an optional next state) so `dhx regen`
//! generates the TLA+ spec (`spec/tla/Lifecycle.tla`) from this Rust — edit the
//! Rust, regen, commit. `Closed` is terminal; only legal transitions return
//! `Some`. Whether an account may take part in a transfer is a function of its
//! state (`State::accepts_transfers`), used by the ledger.

/// The lifecycle state of an account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Open and able to send/receive transfers.
    Open,
    /// Temporarily frozen — rejects all transfers, but may reopen or close.
    Frozen,
    /// Permanently closed — terminal, rejects everything.
    Closed,
}

/// A lifecycle command applied to an account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Freeze an open account.
    Freeze,
    /// Unfreeze a frozen account back to open.
    Unfreeze,
    /// Close the account permanently.
    Close,
}

impl State {
    /// Whether an account in this state may send or receive transfers.
    #[must_use]
    pub fn accepts_transfers(self) -> bool {
        matches!(self, State::Open)
    }
}

/// The lifecycle transition function. Returns the next state for a legal
/// `(state, event)` pair, or `None` when the transition is not allowed (e.g.
/// any event on a `Closed` account, or unfreezing an open one). Total and
/// panic-free.
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "each (state, event) arm is listed explicitly — not merged via an \
              or-pattern — so `dhx regen`'s FSM extractor can read every \
              transition out of the match; merging the two Close arms would hide \
              one transition from the generated TLA+ spec"
)]
pub fn next(state: State, event: Event) -> Option<State> {
    match (state, event) {
        (State::Open, Event::Freeze) => Some(State::Frozen),
        (State::Open, Event::Close) => Some(State::Closed),
        (State::Frozen, Event::Unfreeze) => Some(State::Open),
        (State::Frozen, Event::Close) => Some(State::Closed),
        // Closed is terminal; Open can't be unfrozen; Frozen can't be re-frozen.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{next, Event, State};

    #[test]
    fn legal_transitions() {
        assert_eq!(next(State::Open, Event::Freeze), Some(State::Frozen));
        assert_eq!(next(State::Open, Event::Close), Some(State::Closed));
        assert_eq!(next(State::Frozen, Event::Unfreeze), Some(State::Open));
        assert_eq!(next(State::Frozen, Event::Close), Some(State::Closed));
    }

    #[test]
    fn illegal_transitions_rejected() {
        assert_eq!(next(State::Open, Event::Unfreeze), None);
        assert_eq!(next(State::Frozen, Event::Freeze), None);
        // Closed is terminal — every event is rejected.
        assert_eq!(next(State::Closed, Event::Freeze), None);
        assert_eq!(next(State::Closed, Event::Unfreeze), None);
        assert_eq!(next(State::Closed, Event::Close), None);
    }

    #[test]
    fn only_open_accepts_transfers() {
        assert!(State::Open.accepts_transfers());
        assert!(!State::Frozen.accepts_transfers());
        assert!(!State::Closed.accepts_transfers());
    }

    proptest::proptest! {
        // Closed is an absorbing sink: no event ever leaves it.
        #[test]
        fn closed_is_terminal(e in proptest::sample::select(vec![Event::Freeze, Event::Unfreeze, Event::Close])) {
            proptest::prop_assert_eq!(next(State::Closed, e), None);
        }
    }
}
