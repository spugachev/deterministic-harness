// seats — seat-reservation core for a ticketing service.

/// Lifecycle of a single seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatState {
    Free,
    Held,
    Confirmed,
    Released,
}

/// Events that can drive a seat transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Hold,
    Confirm,
    Release,
    Expire,
}

/// Apply `event` to `state`, returning the next state.
///
/// `Released` is terminal — nothing moves it. Any event that does not make
/// sense for the current state is a no-op (state is returned unchanged).
pub fn next(state: SeatState, event: Event) -> SeatState {
    use Event::*;
    use SeatState::*;
    match (state, event) {
        (Free, Hold) => Held,
        (Held, Confirm) => Confirmed,
        (Held, Release) => Released,
        (Held, Expire) => Free,
        (Confirmed, Release) => Released,
        // Released is terminal; everything else is a no-op.
        (s, _) => s,
    }
}

/// Reserve `qty` seats given `capacity` and the count already `held`.
///
/// Never oversells: if the request would exceed capacity, nothing is reserved
/// and the original held count is returned.
pub fn reserve(capacity: u32, held: u32, qty: u32) -> u32 {
    match held.checked_add(qty) {
        Some(total) if total <= capacity => total,
        _ => held,
    }
}

/// Parse a `"<qty>|<event_name>"` booking string into `(qty, event_name)`.
///
/// On any malformed input, returns `(0, String::new())`.
pub fn parse_booking(s: &str) -> (u32, String) {
    match s.split_once('|') {
        Some((qty, name)) => match qty.trim().parse::<u32>() {
            Ok(q) => (q, name.to_string()),
            Err(_) => (0, String::new()),
        },
        None => (0, String::new()),
    }
}

/// A hold on a seat with an absolute expiry time (opaque tick / epoch unit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hold {
    pub expires_at: u64,
}

/// True when `now` has reached or passed the hold's expiry.
pub fn is_expired(now: u64, hold: Hold) -> bool {
    now >= hold.expires_at
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_transitions() {
        assert_eq!(next(SeatState::Free, Event::Hold), SeatState::Held);
        assert_eq!(next(SeatState::Held, Event::Confirm), SeatState::Confirmed);
        assert_eq!(next(SeatState::Confirmed, Event::Release), SeatState::Released);
    }

    #[test]
    fn reserve_does_not_oversell() {
        assert_eq!(reserve(10, 4, 3), 7);
        assert_eq!(reserve(10, 8, 5), 8); // would oversell -> unchanged
    }

    #[test]
    fn parse_booking_happy_path() {
        assert_eq!(parse_booking("3|hold"), (3, "hold".to_string()));
    }

    #[test]
    fn hold_expiry() {
        let h = Hold { expires_at: 100 };
        assert!(!is_expired(50, h));
        assert!(is_expired(100, h));
    }
}
