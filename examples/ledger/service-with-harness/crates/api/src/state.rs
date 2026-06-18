//! The concurrency adapter: a thread-safe handle around the pure ledger.
//!
//! The domain [`Ledger`] is single-threaded and IO-free. `SharedLedger` is the
//! ONLY place threads meet it — a `Mutex` makes each `transfer` /
//! `apply_lifecycle` atomic, so the conservation / no-overdraft / idempotency
//! laws proven for the pure ledger hold verbatim under concurrency (a transfer
//! reads-checks-writes while holding the lock, so two racers can never both see
//! the pre-debit balance). This is what REQ-006 and the DST exercise.

use std::sync::{Arc, Mutex};

use core::domain::ledger::{Ledger, LifecycleError, TransferError, TransferOutcome};
use core::domain::lifecycle::{Event, State};

/// A cloneable, thread-safe handle to one ledger. Clones share the same state.
#[derive(Clone, Debug, Default)]
pub struct SharedLedger {
    inner: Arc<Mutex<Ledger>>,
}

impl SharedLedger {
    /// A new shared ledger wrapping an empty domain ledger.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Ledger::new())),
        }
    }

    /// Open an account (test/setup helper).
    ///
    /// # Panics
    /// If the lock is poisoned by a panic in another holder (cannot happen — the
    /// domain is panic-free).
    pub fn open_account(&self, id: u64, balance: u64) {
        self.inner
            .lock()
            .expect("ledger lock")
            .open_account(id, balance);
    }

    /// Atomically attempt a transfer. The whole read-check-write is under the
    /// lock, so concurrent callers are serialized — no double-spend.
    ///
    /// # Errors
    /// Propagates the domain [`TransferError`].
    ///
    /// # Panics
    /// If the lock is poisoned (see [`SharedLedger::open_account`]).
    pub fn transfer(
        &self,
        from: u64,
        to: u64,
        amount: u64,
        key: &str,
    ) -> Result<TransferOutcome, TransferError> {
        self.inner
            .lock()
            .expect("ledger lock")
            .transfer(from, to, amount, key)
    }

    /// Atomically apply a lifecycle transition.
    ///
    /// # Errors
    /// Propagates the domain [`LifecycleError`].
    ///
    /// # Panics
    /// If the lock is poisoned (see [`SharedLedger::open_account`]).
    pub fn apply_lifecycle(&self, id: u64, event: Event) -> Result<State, LifecycleError> {
        self.inner
            .lock()
            .expect("ledger lock")
            .apply_lifecycle(id, event)
    }

    /// Query an account's balance and lifecycle state, if it exists.
    ///
    /// # Panics
    /// If the lock is poisoned (see [`SharedLedger::open_account`]).
    #[must_use]
    pub fn query(&self, id: u64) -> Option<(u64, State)> {
        let l = self.inner.lock().expect("ledger lock");
        Some((l.balance(id)?, l.state(id)?))
    }

    /// The conserved total across all accounts (used by the DST invariant).
    ///
    /// # Panics
    /// If the lock is poisoned (see [`SharedLedger::open_account`]).
    #[must_use]
    pub fn total_balance(&self) -> u128 {
        self.inner.lock().expect("ledger lock").total_balance()
    }
}

#[cfg(test)]
mod tests {
    use super::SharedLedger;
    use core::domain::ledger::TransferOutcome;
    use core::domain::lifecycle::{Event, State};

    #[test]
    fn shared_ledger_serializes_a_transfer() {
        let l = SharedLedger::new();
        l.open_account(1, 100);
        l.open_account(2, 0);
        assert_eq!(l.transfer(1, 2, 30, "k"), Ok(TransferOutcome::Applied));
        assert_eq!(l.query(1), Some((70, State::Open)));
        assert_eq!(l.query(2), Some((30, State::Open)));
        assert_eq!(l.query(9), None);
        assert_eq!(l.total_balance(), 100);
    }

    #[test]
    fn shared_ledger_applies_lifecycle() {
        let l = SharedLedger::new();
        l.open_account(1, 0);
        assert_eq!(l.apply_lifecycle(1, Event::Freeze), Ok(State::Frozen));
        assert_eq!(l.query(1), Some((0, State::Frozen)));
    }
}
