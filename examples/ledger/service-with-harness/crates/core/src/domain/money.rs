//! The pure money-movement STEP — the arithmetic heart of a transfer.
//!
//! Modelled as a scalar function over three `u64`s so the conservation /
//! no-overdraft invariant is provable EXHAUSTIVELY by Kani (no symbolic
//! collection, no loop — see CLAUDE.md "Kani proof"). The stateful ledger
//! (`super::ledger`) calls this for the actual balance update; multi-account /
//! multi-step behaviour is covered by proptest + DST, never by Kani.

/// Move `amount` cents from a `from` balance to a `to` balance.
///
/// Returns the `(new_from, new_to)` pair, or `None` when the move is invalid:
/// a zero amount, insufficient funds (no overdraft — `from < amount`), or a
/// `to`-balance overflow. All arithmetic is checked, so this is total and
/// panic-free for every input.
///
/// The law (proved below): when it returns `Some`, money is conserved
/// (`new_from + new_to == from + to`) and never created (`new_from <= from`).
#[must_use]
pub fn apply_transfer(from: u64, to: u64, amount: u64) -> Option<(u64, u64)> {
    if amount == 0 {
        return None;
    }
    let new_from = from.checked_sub(amount)?; // None ⇒ insufficient funds
    let new_to = to.checked_add(amount)?; // None ⇒ would overflow the recipient
    Some((new_from, new_to))
}

#[cfg(test)]
mod tests {
    use super::apply_transfer;

    #[test]
    fn moves_money_and_conserves() {
        assert_eq!(apply_transfer(100, 0, 30), Some((70, 30)));
        assert_eq!(apply_transfer(5, 5, 5), Some((0, 10))); // exact-funds boundary
    }

    #[test]
    fn rejects_invalid_moves() {
        assert_eq!(apply_transfer(100, 0, 0), None); // zero amount
        assert_eq!(apply_transfer(10, 0, 11), None); // insufficient funds
        assert_eq!(apply_transfer(10, u64::MAX, 5), None); // recipient overflow
    }

    proptest::proptest! {
        // Conservation + no-overdraft as a sampled law over the full u64 range.
        #[test]
        fn conserves_and_never_oversells(from in 0_u64.., to in 0_u64.., amount in 0_u64..) {
            if let Some((nf, nt)) = apply_transfer(from, to, amount) {
                proptest::prop_assert_eq!(
                    u128::from(nf) + u128::from(nt),
                    u128::from(from) + u128::from(to)
                );
                proptest::prop_assert!(nf <= from);
                proptest::prop_assert!(nt >= to);
            }
        }
    }
}

// Kani proves conservation + no-overdraft EXHAUSTIVELY over every `(u64,u64,u64)`
// triple. Tractable by construction: three scalar `kani::any()` inputs, pure
// checked arithmetic, NO collection and NO loop — so CBMC never blows up. This is
// the scalar-step shape the CLAUDE.md Kani rules prescribe.
#[cfg(kani)]
mod proofs {
    use super::apply_transfer;

    #[kani::proof]
    fn transfer_conserves_and_never_oversells() {
        let from: u64 = kani::any();
        let to: u64 = kani::any();
        let amount: u64 = kani::any();
        if let Some((new_from, new_to)) = apply_transfer(from, to, amount) {
            // Money is conserved (summed in u128 so the assertion can't overflow).
            assert!(u128::from(new_from) + u128::from(new_to) == u128::from(from) + u128::from(to));
            // Never creates money for the sender (no overdraft).
            assert!(new_from <= from);
            // A non-zero amount actually moved.
            assert!(new_from < from);
        }
    }
}
