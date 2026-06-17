//! Capacity arithmetic — the ONE safety property of this service: **never
//! oversell**. `reserve` is pure, total, and panic-free; for every input the
//! returned new-held count satisfies `held' <= capacity`. This is proven three
//! ways: a proptest law, a `#[kani::proof]` bounded model check, and (by
//! construction) the checked arithmetic below.

/// Why a reservation of `qty` seats was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverError {
    /// Granting `qty` would push `held` above `capacity` (the oversell guard).
    Insufficient {
        /// Seats still available: `capacity - held` (saturating, never negative).
        available: u32,
        /// Seats requested.
        requested: u32,
    },
}

/// Reserve `qty` seats against `capacity`, given `held` already reserved.
///
/// Returns the NEW held count on success. The post-condition — enforced for
/// every possible input, never by assumption — is:
///
/// > the returned `held'` satisfies `held' <= capacity`.
///
/// Totality: all arithmetic is checked/saturating, so there is no input for
/// which this panics or overflows. A request that would oversell (including any
/// call where `held` already exceeds `capacity`) is refused with
/// [`OverError::Insufficient`], leaving the count unchanged.
///
/// # Errors
///
/// Returns [`OverError::Insufficient`] when `held + qty` would exceed
/// `capacity` (reporting the saturating `available` seats and the `requested`
/// amount).
#[must_use = "the reservation result must be checked — ignoring it can oversell"]
pub fn reserve(capacity: u32, held: u32, qty: u32) -> Result<u32, OverError> {
    // Saturating so an already-corrupt `held > capacity` reports 0 available
    // rather than wrapping — it can never manufacture phantom availability.
    let available = capacity.saturating_sub(held);
    // Checked add: if `held + qty` overflows u32 it certainly exceeds capacity,
    // so fold that into the same refusal rather than ever wrapping.
    match held.checked_add(qty) {
        Some(new_held) if new_held <= capacity => Ok(new_held),
        _ => Err(OverError::Insufficient {
            available,
            requested: qty,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserve_within_capacity() {
        assert_eq!(reserve(10, 3, 4), Ok(7));
        assert_eq!(reserve(10, 0, 10), Ok(10)); // fill exactly
        assert_eq!(reserve(10, 10, 0), Ok(10)); // zero-qty no-op at the boundary
    }

    #[test]
    fn reserve_refuses_oversell() {
        assert_eq!(
            reserve(10, 8, 3),
            Err(OverError::Insufficient {
                available: 2,
                requested: 3
            })
        );
    }

    #[test]
    fn reserve_refuses_when_already_over() {
        // Defensive: even a corrupt held > capacity cannot yield availability.
        assert_eq!(
            reserve(5, 9, 1),
            Err(OverError::Insufficient {
                available: 0,
                requested: 1
            })
        );
    }

    #[test]
    fn reserve_handles_overflow_without_panic() {
        assert_eq!(
            reserve(u32::MAX, u32::MAX, 1),
            Err(OverError::Insufficient {
                available: 0,
                requested: 1
            })
        );
    }

    proptest::proptest! {
        // The safety law: a successful reserve NEVER leaves held above capacity,
        // and the returned count accounts for exactly the granted qty. (REQ-002,
        // verified=proptest)
        #[test]
        fn reserve_never_oversells(capacity in 0_u32.., held in 0_u32.., qty in 0_u32..) {
            match reserve(capacity, held, qty) {
                Ok(new_held) => {
                    proptest::prop_assert!(new_held <= capacity);
                    proptest::prop_assert_eq!(new_held, held + qty);
                }
                Err(OverError::Insufficient { available, requested }) => {
                    proptest::prop_assert_eq!(requested, qty);
                    proptest::prop_assert_eq!(available, capacity.saturating_sub(held));
                }
            }
        }

        // Totality: reserve is panic-free for every (capacity, held, qty).
        #[test]
        fn reserve_is_total(capacity in 0_u32.., held in 0_u32.., qty in 0_u32..) {
            let _ = reserve(capacity, held, qty);
        }
    }
}

/// Bounded model check (REQ-002, verified=kani): for ALL `(capacity, held,
/// qty)`, a successful [`reserve`] leaves `held' <= capacity`. Unlike the
/// proptest law (random samples) Kani proves this over the entire symbolic
/// input space — the strongest statement of "never oversell".
#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn reserve_never_oversells() {
        let capacity: u32 = kani::any();
        let held: u32 = kani::any();
        let qty: u32 = kani::any();
        if let Ok(new_held) = reserve(capacity, held, qty) {
            assert!(new_held <= capacity);
            assert!(new_held >= held);
        }
    }
}
