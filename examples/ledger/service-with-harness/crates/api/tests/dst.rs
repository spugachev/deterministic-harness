//! Deterministic simulation test (DST) for REQ-003 / REQ-004 / REQ-006.
//!
//! Drives many transfers — generated from a SEED, so a failure replays exactly —
//! through the locked [`SharedLedger`], some sequentially and some across racing
//! threads. After each run it asserts the safety invariants that must hold under
//! ANY interleaving:
//!   * conservation — the total across all accounts equals the initial total;
//!   * no-overdraft — no balance is ever driven negative (`u64` + the checked
//!     step guarantee this, so the assertion is on the total / per-account sums);
//!   * idempotency — replaying every command by its key moves no further money.
//!
//! Determinism: the schedule is produced by a seeded `SplitMix64` (no thread RNG),
//! so `SEEDS` enumerates reproducible scenarios. The thread interleaving itself
//! is non-deterministic, but the asserted invariants are interleaving-invariant,
//! which is exactly the property under test.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]

use std::sync::Arc;
use std::thread;

use api::state::SharedLedger;

/// Seeds enumerating distinct reproducible schedules. The quick gate runs the
/// first; `--full` sweeps all.
const SEEDS: &[u64] = &[1, 2, 3, 7, 42, 1337, 0xDEAD_BEEF];

const NUM_ACCOUNTS: u64 = 5;
const STARTING_BALANCE: u64 = 1_000;
const OPS_PER_RUN: usize = 200;

/// A deterministic `SplitMix64` stream — seeded, so the whole schedule replays.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }
}

/// One generated transfer command.
#[derive(Clone)]
struct Op {
    from: u64,
    to: u64,
    amount: u64,
    key: String,
}

/// Generate a reproducible batch of transfer ops from a seed. Some keys are
/// intentionally reused so the idempotency path is exercised.
fn generate(seed: u64) -> Vec<Op> {
    let mut rng = Rng(seed);
    let mut ops = Vec::with_capacity(OPS_PER_RUN);
    for i in 0..OPS_PER_RUN {
        let from = rng.below(NUM_ACCOUNTS);
        let to = rng.below(NUM_ACCOUNTS);
        let amount = rng.below(STARTING_BALANCE / 2);
        // Reuse a small key space so ~1/8 of ops collide on a key (replays).
        let key = format!("k{}", rng.below(OPS_PER_RUN as u64 / 8 + 1));
        let _ = i;
        ops.push(Op {
            from,
            to,
            amount,
            key,
        });
    }
    ops
}

fn fresh_ledger() -> SharedLedger {
    let ledger = SharedLedger::new();
    for id in 0..NUM_ACCOUNTS {
        ledger.open_account(id, STARTING_BALANCE);
    }
    ledger
}

fn initial_total() -> u128 {
    u128::from(NUM_ACCOUNTS) * u128::from(STARTING_BALANCE)
}

/// Run a batch concurrently across several threads against one shared ledger.
fn run_concurrently(ledger: &SharedLedger, ops: &[Op]) {
    let threads = 4;
    let chunks: Vec<Vec<Op>> = (0..threads)
        .map(|t| {
            ops.iter()
                .skip(t)
                .step_by(threads)
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect();
    let ledger = Arc::new(ledger.clone());
    let handles: Vec<_> = chunks
        .into_iter()
        .map(|chunk| {
            let ledger = Arc::clone(&ledger);
            thread::spawn(move || {
                for op in chunk {
                    let _ = ledger.transfer(op.from, op.to, op.amount, &op.key);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().expect("worker thread");
    }
}

#[test]
fn conservation_holds_under_concurrent_transfers() {
    for &seed in SEEDS {
        let ledger = fresh_ledger();
        let ops = generate(seed);
        run_concurrently(&ledger, &ops);

        // REQ-003/006: conservation under ANY interleaving.
        assert_eq!(
            ledger.total_balance(),
            initial_total(),
            "conservation violated for seed {seed}"
        );
        // No money was created in any single account (each ≤ the whole total).
        for id in 0..NUM_ACCOUNTS {
            let (bal, _) = ledger.query(id).expect("account exists");
            assert!(
                u128::from(bal) <= initial_total(),
                "account {id} exceeds total for seed {seed}"
            );
        }
    }
}

#[test]
fn replay_is_idempotent_and_conserves() {
    for &seed in SEEDS {
        let ledger = fresh_ledger();
        let ops = generate(seed);

        // Apply once sequentially, snapshot, then replay the SAME ops.
        for op in &ops {
            let _ = ledger.transfer(op.from, op.to, op.amount, &op.key);
        }
        let after_first: Vec<_> = (0..NUM_ACCOUNTS).map(|id| ledger.query(id)).collect();

        for op in &ops {
            let _ = ledger.transfer(op.from, op.to, op.amount, &op.key);
        }
        let after_replay: Vec<_> = (0..NUM_ACCOUNTS).map(|id| ledger.query(id)).collect();

        // REQ-004: replaying applied keys moves no further money.
        assert_eq!(
            after_first, after_replay,
            "replay changed balances for seed {seed}"
        );
        assert_eq!(ledger.total_balance(), initial_total());
    }
}

#[test]
fn sequential_and_concurrent_both_conserve() {
    // A sequential run is a valid interleaving; assert it conserves too, so the
    // invariant isn't accidentally satisfied only by lock contention timing.
    for &seed in SEEDS {
        let ledger = fresh_ledger();
        for op in &generate(seed) {
            let _ = ledger.transfer(op.from, op.to, op.amount, &op.key);
        }
        assert_eq!(ledger.total_balance(), initial_total());
    }
}
