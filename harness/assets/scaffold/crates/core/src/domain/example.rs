//! A throwaway example: clamp a requested amount to a remaining budget.
//!
//! This exists only to keep the freshly-scaffolded project green and to show the
//! spec→code→test shape (REQ-001 + `spec/features/example.feature`). Replace it
//! with your own domain. It is pure, total, and panic-free — overflow-safe by
//! construction — so it satisfies clippy's arithmetic restriction and is a
//! natural proptest/Kani target.

/// Grant at most `remaining` of a `requested` amount; never more than is left.
/// Returns the amount granted (`min(requested, remaining)`).
#[must_use]
pub fn grant(requested: u32, remaining: u32) -> u32 {
    requested.min(remaining)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grants_up_to_remaining() {
        assert_eq!(grant(3, 10), 3); // within budget → full request
        assert_eq!(grant(10, 3), 3); // over budget → clamped to remaining
        assert_eq!(grant(5, 5), 5); // exact boundary
        assert_eq!(grant(0, 0), 0); // nothing requested, nothing left
    }

    proptest::proptest! {
        // The law: a grant is never more than requested, never more than
        // remaining, and is exactly one of the two bounds.
        #[test]
        fn grant_respects_both_bounds(requested in 0_u32.., remaining in 0_u32..) {
            let g = grant(requested, remaining);
            proptest::prop_assert!(g <= requested);
            proptest::prop_assert!(g <= remaining);
            proptest::prop_assert!(g == requested || g == remaining);
        }
    }
}

// Kani proves the same law EXHAUSTIVELY over every `u32` pair (not sampled like
// proptest). This is the TRACTABLE shape to copy: scalar `kani::any()` inputs +
// pure arithmetic, NO symbolic Vec/HashMap and no loops — so CBMC never runs out
// of memory. To prove an invariant over stateful/collection logic, refactor the
// rule into a scalar function like this and prove THAT (see CLAUDE.md "Kani
// proof"); leave the collection to proptest + DST.
#[cfg(kani)]
mod proofs {
    use super::grant;

    #[kani::proof]
    fn grant_respects_both_bounds() {
        let requested: u32 = kani::any();
        let remaining: u32 = kani::any();
        let g = grant(requested, remaining);
        assert!(g <= requested);
        assert!(g <= remaining);
        assert!(g == requested || g == remaining);
    }
}
