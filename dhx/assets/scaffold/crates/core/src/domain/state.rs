//! The lifecycle FSM — the single source of truth `dhx regen` projects into
//! `spec/tla/Lifecycle.{tla,cfg}`. `next` is pure and total.

/// Lifecycle states. `Archived` is terminal (no event leaves it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TodoState {
    /// Open and actionable.
    Active,
    /// Completed; can be reopened or archived.
    Done,
    /// Terminal: no further transitions.
    Archived,
}

/// Lifecycle events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Active → Done.
    Complete,
    /// Done → Active.
    Reopen,
    /// Active/Done → Archived.
    Archive,
}

/// The transition relation. Returns `Some(next)` for a legal `(state, event)`
/// and `None` for an illegal one — total and panic-free, so Kani/Verus can
/// prove it and `dhx regen` can extract it.
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "one explicit arm per (state,event) so `dhx regen` extracts each \
              transition individually — an or-pattern would hide arms from the \
              syn-based FSM extractor"
)]
pub fn next(state: TodoState, event: Event) -> Option<TodoState> {
    match (state, event) {
        (TodoState::Active, Event::Complete) => Some(TodoState::Done),
        (TodoState::Done, Event::Reopen) => Some(TodoState::Active),
        (TodoState::Active, Event::Archive) => Some(TodoState::Archived),
        (TodoState::Done, Event::Archive) => Some(TodoState::Archived),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legal_transitions() {
        assert_eq!(
            next(TodoState::Active, Event::Complete),
            Some(TodoState::Done)
        );
        assert_eq!(
            next(TodoState::Done, Event::Reopen),
            Some(TodoState::Active)
        );
        assert_eq!(
            next(TodoState::Active, Event::Archive),
            Some(TodoState::Archived)
        );
    }

    #[test]
    fn archived_is_terminal() {
        assert_eq!(next(TodoState::Archived, Event::Complete), None);
        assert_eq!(next(TodoState::Archived, Event::Reopen), None);
        assert_eq!(next(TodoState::Archived, Event::Archive), None);
    }

    proptest::proptest! {
        // `next` never panics for any (state, event) — the totality law.
        #[test]
        fn next_is_total(s in 0_u8..3, e in 0_u8..3) {
            let state = [TodoState::Active, TodoState::Done, TodoState::Archived][s as usize];
            let event = [Event::Complete, Event::Reopen, Event::Archive][e as usize];
            let _ = next(state, event);
        }
    }
}
