//! The seat lifecycle FSM — the single source of truth `dhx regen` projects into
//! `spec/tla/Lifecycle.{tla,cfg}`. `next` is pure and total.

/// Seat lifecycle states. `Cancelled` is terminal (no event leaves it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatState {
    /// Available to be held.
    Free,
    /// Held pending confirmation; may expire or be released back to `Free`.
    Held,
    /// Reservation confirmed; may still be released back to `Free`.
    Confirmed,
    /// Terminal: no further transitions.
    Cancelled,
}

/// Seat lifecycle events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatEvent {
    /// Free → Held.
    Hold,
    /// Held → Confirmed.
    Confirm,
    /// Held/Confirmed → Free.
    Release,
    /// Held → Free (the hold timed out).
    Expire,
    /// Held/Confirmed → Cancelled.
    Cancel,
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
pub fn next(state: SeatState, event: SeatEvent) -> Option<SeatState> {
    match (state, event) {
        (SeatState::Free, SeatEvent::Hold) => Some(SeatState::Held),
        (SeatState::Held, SeatEvent::Confirm) => Some(SeatState::Confirmed),
        (SeatState::Held, SeatEvent::Release) => Some(SeatState::Free),
        (SeatState::Held, SeatEvent::Expire) => Some(SeatState::Free),
        (SeatState::Held, SeatEvent::Cancel) => Some(SeatState::Cancelled),
        (SeatState::Confirmed, SeatEvent::Release) => Some(SeatState::Free),
        (SeatState::Confirmed, SeatEvent::Cancel) => Some(SeatState::Cancelled),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATES: [SeatState; 4] = [
        SeatState::Free,
        SeatState::Held,
        SeatState::Confirmed,
        SeatState::Cancelled,
    ];
    const EVENTS: [SeatEvent; 5] = [
        SeatEvent::Hold,
        SeatEvent::Confirm,
        SeatEvent::Release,
        SeatEvent::Expire,
        SeatEvent::Cancel,
    ];

    #[test]
    fn legal_transitions() {
        assert_eq!(
            next(SeatState::Free, SeatEvent::Hold),
            Some(SeatState::Held)
        );
        assert_eq!(
            next(SeatState::Held, SeatEvent::Confirm),
            Some(SeatState::Confirmed)
        );
        assert_eq!(
            next(SeatState::Held, SeatEvent::Release),
            Some(SeatState::Free)
        );
        assert_eq!(
            next(SeatState::Held, SeatEvent::Expire),
            Some(SeatState::Free)
        );
        assert_eq!(
            next(SeatState::Held, SeatEvent::Cancel),
            Some(SeatState::Cancelled)
        );
        assert_eq!(
            next(SeatState::Confirmed, SeatEvent::Release),
            Some(SeatState::Free)
        );
        assert_eq!(
            next(SeatState::Confirmed, SeatEvent::Cancel),
            Some(SeatState::Cancelled)
        );
    }

    #[test]
    fn cancelled_is_terminal() {
        for event in EVENTS {
            assert_eq!(next(SeatState::Cancelled, event), None);
        }
    }

    #[test]
    fn free_only_holds() {
        // Free is inert to everything except Hold (no confirm/release/cancel).
        assert_eq!(next(SeatState::Free, SeatEvent::Confirm), None);
        assert_eq!(next(SeatState::Free, SeatEvent::Release), None);
        assert_eq!(next(SeatState::Free, SeatEvent::Expire), None);
        assert_eq!(next(SeatState::Free, SeatEvent::Cancel), None);
    }

    proptest::proptest! {
        // `next` never panics for any (state, event) — the totality law.
        #[test]
        fn next_is_total(s in 0_usize..STATES.len(), e in 0_usize..EVENTS.len()) {
            let _ = next(STATES[s], EVENTS[e]);
        }
    }
}
