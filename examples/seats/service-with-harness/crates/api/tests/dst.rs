//! Deterministic simulation test (REQ-007).
//!
//! Drives randomized hold/confirm/release/availability sequences against the
//! REAL serialized `AppState` under a SEEDED deterministic clock that only
//! advances forward, asserting the no-overbooking invariant after every step.
//! A failure prints its seed and replays identically.
//!
//! `harness = false`: this test owns `fn main`, so we drive seeds ourselves
//! (libtest would otherwise run "0 tests" and skip the simulation vacuously).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test binary: panics ARE the failure signal and replay deterministically"
)]

use core::domain::seats::HoldError;
use core::ports::{Clock, IdGen};
use std::cell::Cell;

/// A deterministic clock that advances forward by a seeded amount each step.
/// Interior mutability so the immutable-`&self` `Clock` port can still tick.
struct SimClock {
    now: Cell<i64>,
}

impl Clock for SimClock {
    fn now_unix(&self) -> i64 {
        self.now.get()
    }
}

impl SimClock {
    fn advance(&self, dt: i64) {
        self.now.set(self.now.get().saturating_add(dt));
    }
}

/// A simple deterministic id source (SplitMix-free monotone counter).
struct SimIds(u64);

impl IdGen for SimIds {
    fn next_id(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1);
        self.0
    }
}

/// A tiny seeded xorshift PRNG — deterministic, no external crate.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// A small `u32` in `0..n` (n is tiny, so the conversion is lossless).
    fn below_u32(&mut self, n: u32) -> u32 {
        u32::try_from(self.below(u64::from(n))).unwrap_or(0)
    }

    /// A small forward time step in `0..n` seconds as `i64` (lossless).
    fn below_i64(&mut self, n: u64) -> i64 {
        i64::try_from(self.below(n)).unwrap_or(0)
    }

    /// An index into a non-empty slice (lossless: bounded by `len`).
    fn index(&mut self, len: usize) -> usize {
        let len64 = u64::try_from(len).unwrap_or(u64::MAX).max(1);
        usize::try_from(self.below(len64)).unwrap_or(0)
    }
}

use api::state::{AppState, DEFAULT_TTL_SECS};

/// Run one seeded simulation: `steps` operations against a venue, checking the
/// invariant `occupied <= capacity` (encoded as `available <= capacity`) after
/// each. Returns the number of granted holds (sanity: the sim does real work).
fn run_seed(seed: u64, steps: u32) -> u64 {
    let capacity = 1 + u32::try_from(seed % 8).unwrap_or(0); // 1..=8 seats
    let clock = SimClock { now: Cell::new(0) };
    // We reuse the clock by reference inside AppState via a wrapper below.
    let state = AppState::new(capacity, ClockRef(&clock), SimIds(0), DEFAULT_TTL_SECS);
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1));
    // Track ids we've seen granted so confirm/release sometimes hit live ones.
    let mut known: Vec<u64> = Vec::new();
    let mut grants = 0_u64;

    for _ in 0..steps {
        // The clock only ever moves forward (the monotonicity ADR-0001 relies on).
        clock.advance(rng.below_i64(4)); // 0..=3 seconds
        match rng.below(4) {
            0 => {
                let seats = 1 + rng.below_u32(4); // 1..=4
                match state.hold(seats) {
                    Ok(g) => {
                        grants += 1;
                        known.push(g.id);
                    }
                    // Rejections are expected outcomes, not failures.
                    Err(
                        HoldError::InsufficientAvailability
                        | HoldError::ZeroSeats
                        | HoldError::NotHeld,
                    ) => {}
                }
            }
            1 => {
                let id = pick(&mut rng, &known);
                let _ = state.confirm(id);
            }
            2 => {
                let id = pick(&mut rng, &known);
                state.release(id);
            }
            _ => {
                let _ = state.available();
            }
        }
        // THE invariant (REQ-007): confirmed + live-held seats never exceed
        // capacity. `available` saturates at zero and so cannot witness an
        // overbooking; `occupied` is the raw count, so assert on THAT.
        let occupied = state.occupied();
        assert!(
            occupied <= capacity,
            "seed {seed}: occupied {occupied} exceeds capacity {capacity} — overbooked",
        );
    }
    grants
}

/// Wrapper letting `AppState` own a `Clock` that borrows the sim clock.
struct ClockRef<'a>(&'a SimClock);

impl Clock for ClockRef<'_> {
    fn now_unix(&self) -> i64 {
        self.0.now_unix()
    }
}

fn pick(rng: &mut Rng, known: &[u64]) -> u64 {
    if known.is_empty() {
        // An id that was never granted — exercises the unknown/no-op paths.
        rng.next()
    } else {
        let i = rng.index(known.len());
        known.get(i).copied().unwrap_or(0)
    }
}

fn main() {
    // Sweep a band of seeds; one DST seed in `verify --quick`, the full sweep
    // in `verify --full`. Each is fully deterministic and replayable.
    let seeds: u64 = std::env::var("DST_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(64);
    let steps: u32 = 200;
    let mut total_grants = 0_u64;
    for seed in 0..seeds {
        total_grants += run_seed(seed, steps);
    }
    // Sanity: across the sweep the simulation actually granted holds (the
    // invariant check isn't passing vacuously on an all-rejection run).
    assert!(total_grants > 0, "DST did no real work: zero holds granted");
    println!("DST ok: {seeds} seeds x {steps} steps, {total_grants} holds granted");
}
