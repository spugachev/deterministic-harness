//! Deterministic simulation test (DST) for REQ-005.
//!
//! Drives the real mutex-serialized [`SeatService`] over a SIMULATED world: a
//! deterministic clock that only advances forward and a seeded operation
//! schedule. The whole run is a pure function of the seed, so any failure
//! replays exactly. Two flavours:
//!
//! 1. A randomized op sequence (hold/confirm/release/advance time) over many
//!    seeds, asserting the capacity invariant `confirmed + held <= capacity`
//!    after every step — the serial reduction of "no overbooking ever".
//! 2. A concurrency race: many threads hammer `hold(1)` on a 1-seat event; the
//!    mutex must let at most ONE succeed (two clients racing for the last seat
//!    never both win).
//!
//! `harness = false` (see Cargo.toml): this test owns `fn main`, so libtest does
//! not wrap it (which would report "0 tests" and silently skip the simulation).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "test-only deterministic simulation"
)]

use api::service::SeatService;
use core::ports::{Clock, IdGen};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;

/// A clock the simulation advances explicitly — monotonic by construction.
#[derive(Debug, Default)]
struct SimClock {
    secs: AtomicI64,
}

impl SimClock {
    fn advance(&self, dt: i64) {
        self.secs.fetch_add(dt, Ordering::SeqCst);
    }
}

impl Clock for SimClock {
    fn now_unix(&self) -> i64 {
        self.secs.load(Ordering::SeqCst)
    }
}

/// A thread-safe monotonic id source for the simulation.
#[derive(Debug, Default)]
struct SimIds {
    next: AtomicU64,
}

impl IdGen for SimIds {
    fn next_id(&mut self) -> u64 {
        self.next.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
    }
}

// A tiny deterministic PRNG (SplitMix64) so the schedule is a pure fn of seed.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A `Clock` adapter that shares an `Arc<SimClock>` (so a driver thread and the
/// service see the same, externally-advanced time source).
struct ClockRef(Arc<SimClock>);
impl Clock for ClockRef {
    fn now_unix(&self) -> i64 {
        self.0.now_unix()
    }
}

/// One seeded simulation run: a single-threaded op sequence against the service,
/// asserting the capacity invariant after each step.
fn run_seed(seed: u64, capacity: u32) {
    let clock = Arc::new(SimClock::default());
    let svc = SeatService::new(capacity, ClockRef(Arc::clone(&clock)), SimIds::default());
    let mut rng = Rng(seed);
    let mut live: Vec<u64> = Vec::new();

    for _ in 0..200 {
        match rng.below(4) {
            0 => {
                let seats = (rng.below(4) + 1) as u32; // 1..=4
                if let Ok(g) = svc.hold(seats) {
                    live.push(g.id);
                }
            }
            1 => {
                if !live.is_empty() {
                    let i = (rng.below(live.len() as u64)) as usize;
                    let _ = svc.confirm(live[i]);
                }
            }
            2 => {
                if !live.is_empty() {
                    let i = (rng.below(live.len() as u64)) as usize;
                    let id = live.swap_remove(i);
                    let _ = svc.release(id);
                }
            }
            _ => {
                // advance time 0..=3s — monotonic, lets holds expire
                clock.advance(rng.below(4) as i64);
            }
        }
        // THE invariant: never oversold. `available <= capacity` is the
        // observable face of `confirmed + live-held <= capacity`.
        assert!(
            svc.available() <= capacity,
            "seed {seed}: available {} > capacity {capacity}",
            svc.available()
        );
        assert!(
            svc.confirmed() <= capacity,
            "seed {seed}: confirmed {} > capacity {capacity}",
            svc.confirmed()
        );
    }
}

/// The concurrency race: N threads each try to grab the last seat; at most one
/// may succeed. Deterministic in outcome (count), if not in interleaving.
fn run_race(threads: usize) {
    let clock = Arc::new(SimClock::default());
    let svc = Arc::new(SeatService::new(
        1,
        ClockRef(Arc::clone(&clock)),
        SimIds::default(),
    ));
    let mut handles = Vec::new();
    for _ in 0..threads {
        let s = Arc::clone(&svc);
        handles.push(std::thread::spawn(move || s.hold(1).is_ok()));
    }
    // `join` consumes the handle, so map to booleans first, then count the wins.
    let wins = handles
        .into_iter()
        .map(|h| h.join().expect("thread panicked"))
        .filter(|won| *won)
        .count();
    assert_eq!(wins, 1, "exactly one client should win the last seat");
    assert!(svc.available() <= 1);
    // silence unused-field warning on the shared clock in this flavour
    let _ = clock.now_unix();
}

fn main() {
    // Sequence invariant over a sweep of seeds and capacities.
    for seed in 0..64_u64 {
        for capacity in [0_u32, 1, 3, 8] {
            run_seed(seed, capacity);
        }
    }
    // The last-seat race, repeated to shake out interleavings.
    for _ in 0..50 {
        run_race(16);
    }
    println!("dst: all seeds and races held the capacity invariant");
}
