//! The transfer ledger — the stateful (but still IO-free) aggregate.
//!
//! Holds integer-cent accounts keyed by id, each with a balance and a lifecycle
//! [`State`](super::lifecycle::State). Applies transfers atomically with respect
//! to its own state, enforces no-overdraft / conservation via
//! [`apply_transfer`](super::money::apply_transfer), and de-duplicates by
//! idempotency key. There is NO async, no lock, no IO here: concurrency is added
//! by an outer adapter that wraps this behind a `Mutex` (see the `api` crate),
//! so the conservation/idempotency laws are testable purely (proptest) and
//! deterministically (DST) before any threads are involved.

use std::collections::BTreeMap;

use super::lifecycle::{self, Event, State};
use super::money::apply_transfer;

/// One account: an integer-cent balance and its lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Account {
    /// Balance in integer cents (never negative — `u64`).
    pub balance: u64,
    /// Lifecycle state.
    pub state: State,
}

/// The outcome of attempting a transfer. Stored per idempotency key so a replay
/// returns the original result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferOutcome {
    /// Money moved: `amount` cents from `from` to `to`.
    Applied,
    /// Replay of an already-applied key — no state changed this time.
    Duplicate,
}

/// Why a transfer was rejected (typed; no state change occurred).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferError {
    /// `from == to` — a self-transfer is not allowed.
    SelfTransfer,
    /// The source or destination account does not exist.
    NoSuchAccount,
    /// The source account cannot send (frozen/closed).
    SourceNotOpen,
    /// The destination account cannot receive (frozen/closed).
    DestNotOpen,
    /// Amount was zero, exceeded the source balance, or overflowed the dest.
    InvalidAmount,
}

/// Why a lifecycle command was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleError {
    /// The account does not exist.
    NoSuchAccount,
    /// The transition is illegal from the account's current state.
    IllegalTransition,
}

/// An in-memory ledger of accounts plus the set of applied idempotency keys.
#[derive(Clone, Debug, Default)]
pub struct Ledger {
    accounts: BTreeMap<u64, Account>,
    applied_keys: BTreeMap<String, TransferOutcome>,
}

impl Ledger {
    /// A new empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a new account with `id` and an initial `balance`. Replaces any
    /// existing account at `id` (used for setup/tests).
    pub fn open_account(&mut self, id: u64, balance: u64) {
        self.accounts.insert(
            id,
            Account {
                balance,
                state: State::Open,
            },
        );
    }

    /// The current balance of `id`, if the account exists.
    #[must_use]
    pub fn balance(&self, id: u64) -> Option<u64> {
        self.accounts.get(&id).map(|a| a.balance)
    }

    /// The current lifecycle state of `id`, if the account exists.
    #[must_use]
    pub fn state(&self, id: u64) -> Option<State> {
        self.accounts.get(&id).map(|a| a.state)
    }

    /// The sum of every account balance — the conserved quantity.
    #[must_use]
    pub fn total_balance(&self) -> u128 {
        self.accounts.values().map(|a| u128::from(a.balance)).sum()
    }

    /// Apply a lifecycle `event` to account `id` via the pure FSM.
    ///
    /// # Errors
    /// [`LifecycleError::NoSuchAccount`] if `id` is unknown;
    /// [`LifecycleError::IllegalTransition`] if the FSM rejects the transition.
    pub fn apply_lifecycle(&mut self, id: u64, event: Event) -> Result<State, LifecycleError> {
        let account = self
            .accounts
            .get_mut(&id)
            .ok_or(LifecycleError::NoSuchAccount)?;
        let next =
            lifecycle::next(account.state, event).ok_or(LifecycleError::IllegalTransition)?;
        account.state = next;
        Ok(next)
    }

    /// Attempt a transfer of `amount` cents from `from` to `to`, de-duplicated
    /// by `key`.
    ///
    /// Idempotent: the first call with a given `key` performs the move and
    /// records it; any later call with the same `key` is a no-op that returns
    /// [`TransferOutcome::Duplicate`] — money moves at most once per key. A
    /// rejected transfer changes no state and is NOT recorded (so the key may be
    /// retried).
    ///
    /// # Errors
    /// A [`TransferError`] describing why the move was rejected; no state changes
    /// in that case.
    pub fn transfer(
        &mut self,
        from: u64,
        to: u64,
        amount: u64,
        key: &str,
    ) -> Result<TransferOutcome, TransferError> {
        // Replay short-circuit: an already-applied key never moves money again.
        if self.applied_keys.contains_key(key) {
            return Ok(TransferOutcome::Duplicate);
        }

        if from == to {
            return Err(TransferError::SelfTransfer);
        }

        // Read both accounts (no mutation yet — a rejected transfer must be a
        // total no-op, so all checks happen before any write).
        let from_acct = *self
            .accounts
            .get(&from)
            .ok_or(TransferError::NoSuchAccount)?;
        let to_acct = *self.accounts.get(&to).ok_or(TransferError::NoSuchAccount)?;

        if !from_acct.state.accepts_transfers() {
            return Err(TransferError::SourceNotOpen);
        }
        if !to_acct.state.accepts_transfers() {
            return Err(TransferError::DestNotOpen);
        }

        let (new_from, new_to) = apply_transfer(from_acct.balance, to_acct.balance, amount)
            .ok_or(TransferError::InvalidAmount)?;

        // Commit: both writes succeed together (single-threaded here; the outer
        // adapter holds a lock across this whole method).
        self.accounts.insert(
            from,
            Account {
                balance: new_from,
                state: from_acct.state,
            },
        );
        self.accounts.insert(
            to,
            Account {
                balance: new_to,
                state: to_acct.state,
            },
        );
        self.applied_keys
            .insert(key.into(), TransferOutcome::Applied);
        Ok(TransferOutcome::Applied)
    }

    /// All `(id, account)` pairs, ascending by id — for queries/snapshots.
    #[must_use]
    pub fn snapshot(&self) -> Vec<(u64, Account)> {
        self.accounts.iter().map(|(&id, &a)| (id, a)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{Event, Ledger, State, TransferError, TransferOutcome};

    fn two_accounts() -> Ledger {
        let mut l = Ledger::new();
        l.open_account(1, 100);
        l.open_account(2, 0);
        l
    }

    #[test]
    fn happy_path_moves_money_and_conserves() {
        let mut l = two_accounts();
        let before = l.total_balance();
        assert_eq!(l.transfer(1, 2, 30, "k1"), Ok(TransferOutcome::Applied));
        assert_eq!(l.balance(1), Some(70));
        assert_eq!(l.balance(2), Some(30));
        assert_eq!(l.total_balance(), before);
    }

    #[test]
    fn idempotent_replay_moves_money_once() {
        let mut l = two_accounts();
        assert_eq!(l.transfer(1, 2, 30, "k1"), Ok(TransferOutcome::Applied));
        // Same key again: no-op, balances unchanged.
        assert_eq!(l.transfer(1, 2, 30, "k1"), Ok(TransferOutcome::Duplicate));
        assert_eq!(l.balance(1), Some(70));
        assert_eq!(l.balance(2), Some(30));
    }

    #[test]
    fn rejections_change_nothing() {
        let mut l = two_accounts();
        let before = l.total_balance();
        assert_eq!(l.transfer(1, 1, 10, "s"), Err(TransferError::SelfTransfer));
        assert_eq!(l.transfer(1, 2, 0, "z"), Err(TransferError::InvalidAmount));
        assert_eq!(
            l.transfer(1, 2, 9999, "o"),
            Err(TransferError::InvalidAmount)
        );
        assert_eq!(l.transfer(1, 9, 5, "n"), Err(TransferError::NoSuchAccount));
        assert_eq!(l.balance(1), Some(100));
        assert_eq!(l.total_balance(), before);
        // A rejected key was not recorded — it can be retried successfully.
        assert_eq!(l.transfer(1, 2, 10, "z"), Ok(TransferOutcome::Applied));
    }

    #[test]
    fn frozen_and_closed_reject_transfers_both_ways() {
        let mut l = two_accounts();
        l.open_account(3, 50);
        assert_eq!(l.apply_lifecycle(1, Event::Freeze), Ok(State::Frozen));
        assert_eq!(l.transfer(1, 2, 5, "a"), Err(TransferError::SourceNotOpen));
        // A frozen account can't receive either.
        assert_eq!(l.transfer(3, 1, 5, "c"), Err(TransferError::DestNotOpen));
        assert_eq!(l.apply_lifecycle(1, Event::Close), Ok(State::Closed));
        assert_eq!(l.transfer(3, 1, 5, "d"), Err(TransferError::DestNotOpen));
    }

    #[test]
    fn query_distinguishes_unknown_from_empty() {
        let l = two_accounts(); // account 2 has balance 0
        assert_eq!(l.balance(2), Some(0)); // exists, empty (REQ-007)
        assert_eq!(l.balance(99), None); // unknown — not a fabricated zero
        assert_eq!(l.state(99), None);
    }

    proptest::proptest! {
        // REQ-003: conservation. Whatever an arbitrary transfer attempt does
        // (apply / duplicate / reject), the SUM of balances is invariant.
        #[test]
        fn total_balance_invariant_under_any_transfer(
            a in 0_u64..1_000_000, b in 0_u64..1_000_000,
            from in 1_u64..=3, to in 1_u64..=3, amount in 0_u64..2_000_000,
        ) {
            let mut l = Ledger::new();
            l.open_account(1, a);
            l.open_account(2, b);
            // account 3 deliberately absent → exercises NoSuchAccount paths.
            let before = l.total_balance();
            let _ = l.transfer(from, to, amount, "k");
            proptest::prop_assert_eq!(l.total_balance(), before);
            // No balance ever exceeds the conserved total (no money created).
            for (_, acct) in l.snapshot() {
                proptest::prop_assert!(u128::from(acct.balance) <= before);
            }
        }

        // REQ-004: idempotency. Re-submitting the same key never moves money a
        // second time — the post-replay balances equal the post-first balances.
        #[test]
        fn replay_moves_money_at_most_once(
            a in 0_u64..1_000_000, amount in 0_u64..1_000_000,
        ) {
            let mut l = Ledger::new();
            l.open_account(1, a);
            l.open_account(2, 0);
            let first = l.transfer(1, 2, amount, "dup");
            let (b1, b2) = (l.balance(1), l.balance(2));
            let second = l.transfer(1, 2, amount, "dup");
            // Replay yields the SAME balances; if the first applied, the second
            // is a Duplicate (never a second Applied).
            proptest::prop_assert_eq!(l.balance(1), b1);
            proptest::prop_assert_eq!(l.balance(2), b2);
            if first == Ok(TransferOutcome::Applied) {
                proptest::prop_assert_eq!(second, Ok(TransferOutcome::Duplicate));
            }
        }
    }
}
