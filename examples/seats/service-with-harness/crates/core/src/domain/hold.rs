//! The hold lifecycle as a pure state machine (REQ-002).
//!
//! A single hold moves through a tiny FSM: it is born `Held`, then exactly one
//! terminal transition fires — `Confirm` → `Confirmed`, `Release` → `Released`,
//! or `Expire` → `Expired`. Terminal states accept no further events (you
//! cannot confirm a released hold, re-confirm a confirmed one, etc.).
//!
//! `next` is a total pure function `fn(State, Event) -> Option<State>`; `None`
//! means "this event is not legal in this state". `harness.toml`'s `[fsm]`
//! section points `dhx regen` at this function, which projects it to
//! `spec/tla/Lifecycle.tla` so TLC model-checks the same transition relation
//! the Rust executes.

/// Lifecycle state of one hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Live, unconfirmed reservation awaiting confirm/release/expiry.
    Held,
    /// Permanently booked — terminal.
    Confirmed,
    /// Voluntarily returned before expiry — terminal.
    Released,
    /// TTL elapsed without confirmation — terminal.
    Expired,
}

/// Events that drive a hold's lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The client confirmed the hold in time.
    Confirm,
    /// The client released the hold.
    Release,
    /// The TTL elapsed.
    Expire,
}

/// The lifecycle transition relation. Returns the next state, or `None` when
/// the event is illegal in the current state (every terminal state rejects all
/// events — there is no way back out of `Confirmed`/`Released`/`Expired`).
#[must_use]
pub fn next(state: State, event: Event) -> Option<State> {
    match (state, event) {
        (State::Held, Event::Confirm) => Some(State::Confirmed),
        (State::Held, Event::Release) => Some(State::Released),
        (State::Held, Event::Expire) => Some(State::Expired),
        // Confirmed, Released, Expired are terminal: no event is legal.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_accepts_each_terminal_transition() {
        assert_eq!(next(State::Held, Event::Confirm), Some(State::Confirmed));
        assert_eq!(next(State::Held, Event::Release), Some(State::Released));
        assert_eq!(next(State::Held, Event::Expire), Some(State::Expired));
    }

    #[test]
    fn terminal_states_reject_all_events() {
        for state in [State::Confirmed, State::Released, State::Expired] {
            for event in [Event::Confirm, Event::Release, Event::Expire] {
                assert_eq!(next(state, event), None, "{state:?} + {event:?}");
            }
        }
    }

    proptest::proptest! {
        // A transition out of Held always lands in a DISTINCT terminal state,
        // and no transition ever returns to Held.
        #[test]
        fn transitions_are_one_way(s in 0_u8..4, e in 0_u8..3) {
            let state = [State::Held, State::Confirmed, State::Released, State::Expired][s as usize];
            let event = [Event::Confirm, Event::Release, Event::Expire][e as usize];
            if let Some(after) = next(state, event) {
                proptest::prop_assert_eq!(state, State::Held);
                proptest::prop_assert_ne!(after, State::Held);
            }
        }
    }
}
