//! Hold expiry — when a `Held` seat times out. Reads "now" through the
//! [`Clock`](crate::ports::Clock) port (never `SystemTime::now`, which
//! `clippy.toml` bans) so the check is deterministic and testable under DST.

use crate::ports::Clock;

/// A seat hold: it expires at `expires_at_unix` (whole seconds since the Unix
/// epoch).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hold {
    /// The instant, in Unix seconds, at and after which the hold is expired.
    pub expires_at_unix: i64,
}

/// Has `hold` expired as of the clock's current time?
///
/// Expiry is inclusive of the boundary: a hold is expired exactly when
/// `now >= expires_at`. Pure and total — it only reads the clock, so the same
/// clock value always yields the same answer.
#[must_use]
pub fn is_expired(clock: &impl Clock, hold: Hold) -> bool {
    clock.now_unix() >= hold.expires_at_unix
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::FixedClock;
    use proptest::prelude::any;

    #[test]
    fn not_expired_before_deadline() {
        let clock = FixedClock(100);
        assert!(!is_expired(
            &clock,
            Hold {
                expires_at_unix: 101
            }
        ));
    }

    #[test]
    fn expired_at_and_after_deadline() {
        assert!(is_expired(
            &FixedClock(101),
            Hold {
                expires_at_unix: 101
            }
        ));
        assert!(is_expired(
            &FixedClock(200),
            Hold {
                expires_at_unix: 101
            }
        ));
    }

    proptest::proptest! {
        // Monotonicity law (REQ-003): once expired at some time, a hold stays
        // expired at every later time — expiry never flips back to live.
        #[test]
        fn expiry_is_monotonic(expires_at in i64::MIN..i64::MAX, now in any::<i64>(), delta in 0_i64..) {
            let hold = Hold { expires_at_unix: expires_at };
            let later = now.saturating_add(delta);
            if is_expired(&FixedClock(now), hold) {
                proptest::prop_assert!(is_expired(&FixedClock(later), hold));
            }
        }

        // The boundary is exactly `now >= expires_at` for every input.
        #[test]
        fn expiry_matches_comparison(expires_at in any::<i64>(), now in any::<i64>()) {
            proptest::prop_assert_eq!(
                is_expired(&FixedClock(now), Hold { expires_at_unix: expires_at }),
                now >= expires_at
            );
        }
    }
}
