//! The hold lifecycle as a pure state machine.
//!
//! A hold moves from Held to one of Confirmed, Released, or Expired.
//! Confirmed/Released/Expired are terminal - no event applies to them, so `next`
//! returns `None`. This is the pure transition function that the harness.toml
//! fsm section points at; `dhx regen` projects it to spec/tla/Lifecycle.tla and
//! TLC model-checks the lifecycle.
//!
//! Seat-accounting lives in the `seats` module; this FSM governs only the
//! per-hold status transitions, which is what makes "confirm an expired hold is
//! rejected" a structural fact rather than an arithmetic one.
//!
//! NB: keep the doc-comments in this file plain ASCII without square brackets;
//! the FSM extractor parses them and rejects non-ASCII or link syntax.

/// Lifecycle state of a single hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// The hold is outstanding and may still be confirmed or released.
    Held,
    /// The hold was confirmed - its seats are permanently booked. Terminal.
    Confirmed,
    /// The hold was released by the client before expiry. Terminal.
    Released,
    /// The hold's TTL elapsed before confirmation. Terminal.
    Expired,
}

/// An event that may drive a hold's lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The client confirms the hold before it expires.
    Confirm,
    /// The client releases the hold before it expires.
    Release,
    /// The TTL elapses with the hold still outstanding.
    Expire,
}

/// The pure transition function: given the current state and an event, return
/// the next state, or `None` if the event does not apply (terminal state).
///
/// Only Held accepts events; every terminal state rejects every event, so the
/// machine has no transitions out of Confirmed/Released/Expired.
#[must_use]
pub const fn next(state: State, event: Event) -> Option<State> {
    match (state, event) {
        (State::Held, Event::Confirm) => Some(State::Confirmed),
        (State::Held, Event::Release) => Some(State::Released),
        (State::Held, Event::Expire) => Some(State::Expired),
        // Terminal states accept no events.
        (State::Confirmed | State::Released | State::Expired, _) => None,
    }
}

/// Is `state` terminal (no further transitions possible)?
#[must_use]
pub const fn is_terminal(state: State) -> bool {
    matches!(state, State::Confirmed | State::Released | State::Expired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_accepts_each_event_to_its_terminal() {
        assert_eq!(next(State::Held, Event::Confirm), Some(State::Confirmed));
        assert_eq!(next(State::Held, Event::Release), Some(State::Released));
        assert_eq!(next(State::Held, Event::Expire), Some(State::Expired));
    }

    #[test]
    fn terminal_states_reject_all_events() {
        for s in [State::Confirmed, State::Released, State::Expired] {
            for e in [Event::Confirm, Event::Release, Event::Expire] {
                assert_eq!(next(s, e), None, "{s:?} should reject {e:?}");
            }
        }
        assert!(is_terminal(State::Confirmed));
        assert!(is_terminal(State::Released));
        assert!(is_terminal(State::Expired));
        assert!(!is_terminal(State::Held));
    }

    proptest::proptest! {
        // A transition out of Held always lands in a terminal state; no event
        // ever applies to a terminal state. (Totality + terminality law.)
        #[test]
        fn transitions_only_from_held_and_into_terminal(
            si in 0_usize..4,
            ei in 0_usize..3,
        ) {
            let states = [State::Held, State::Confirmed, State::Released, State::Expired];
            let events = [Event::Confirm, Event::Release, Event::Expire];
            let s = states[si];
            let e = events[ei];
            match next(s, e) {
                Some(ns) => {
                    proptest::prop_assert_eq!(s, State::Held);
                    proptest::prop_assert!(is_terminal(ns));
                }
                None => proptest::prop_assert!(is_terminal(s)),
            }
        }
    }
}
