//! The hold lifecycle as a pure finite-state machine.
//!
//! A hold is born `Held` and ends in exactly one terminal state: `Confirmed`
//! (the client booked it), `Released` (the client gave it back), or `Expired`
//! (its TTL elapsed before confirmation). The transition table is the single
//! pure function [`next`]; `harness.toml`'s `[fsm]` section points `dhx regen`
//! at it so the TLA+ lifecycle spec (`spec/tla/Lifecycle.tla`) is GENERATED from
//! this Rust — edit the table here, `dhx regen`, commit. The terminal states
//! have no outgoing transition, which the generated `ArchivedTerminal` invariant
//! checks (REQ-002 / REQ-003 / REQ-004).

/// The lifecycle state of a single hold. The first variant (`Held`) is the
/// initial state the generated TLA+ `Init` pins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldState {
    /// Seats are reserved but not yet booked; the only non-terminal state.
    Held,
    /// The hold was confirmed before expiry — its seats are permanently booked.
    Confirmed,
    /// The hold was released by the client before expiry — seats returned.
    Released,
    /// The hold's TTL elapsed before confirmation — seats freed automatically.
    Expired,
}

/// An event that can act on a hold. Only `Held` reacts to any of these; every
/// terminal state ignores all events (the FSM returns `None`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldEvent {
    /// The client confirms the hold, booking its seats.
    Confirm,
    /// The client releases the hold, returning its seats.
    Release,
    /// The hold's TTL elapsed; the system expires it lazily.
    Expire,
}

/// The pure transition table. Returns `Some(next_state)` for a legal
/// `(state, event)` and `None` when the event is not enabled in that state
/// (every terminal state is a sink). Total and panic-free — a natural TLA+ /
/// proptest target. `dhx regen` extracts the `Some`-returning arms to generate
/// `spec/tla/Lifecycle.tla`.
#[must_use]
pub fn next(state: HoldState, event: HoldEvent) -> Option<HoldState> {
    match (state, event) {
        (HoldState::Held, HoldEvent::Confirm) => Some(HoldState::Confirmed),
        (HoldState::Held, HoldEvent::Release) => Some(HoldState::Released),
        (HoldState::Held, HoldEvent::Expire) => Some(HoldState::Expired),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{next, HoldEvent, HoldState};

    #[test]
    fn held_transitions_to_each_terminal() {
        assert_eq!(
            next(HoldState::Held, HoldEvent::Confirm),
            Some(HoldState::Confirmed)
        );
        assert_eq!(
            next(HoldState::Held, HoldEvent::Release),
            Some(HoldState::Released)
        );
        assert_eq!(
            next(HoldState::Held, HoldEvent::Expire),
            Some(HoldState::Expired)
        );
    }

    #[test]
    fn terminal_states_are_sinks() {
        // No event is enabled in any terminal state.
        for state in [
            HoldState::Confirmed,
            HoldState::Released,
            HoldState::Expired,
        ] {
            for event in [HoldEvent::Confirm, HoldEvent::Release, HoldEvent::Expire] {
                assert_eq!(next(state, event), None, "{state:?} should be terminal");
            }
        }
    }
}
