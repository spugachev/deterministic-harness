//! Deterministic simulation test (DST) for the seat-reservation service.
//!
//! A single seed drives a long pseudo-random sequence of hold/confirm/release
//! operations and clock advances against the real [`Reservation`], with the
//! capacity invariant (REQ-005) asserted after every step. A failure prints the
//! seed so it replays deterministically:
//!
//!   `DST_SEED=<n> DST_ITERATIONS=<m> cargo test -p api --test dst -- --nocapture`
//!
//! `dhx verify` sets `DST_SEED`/`DST_ITERATIONS`; absent, small defaults run so
//! `cargo test` alone still exercises it.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test-only"
)]

use seats_core::domain::reservation::Reservation;
use seats_core::ports::{Clock, Rng, SeqGen};

/// A clock the simulation can advance deterministically.
struct SimClock(i64);
impl Clock for SimClock {
    fn now_unix(&self) -> i64 {
        self.0
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[test]
fn capacity_invariant_holds_under_simulation() {
    let seed = env_u64("DST_SEED", 0);
    let iterations = env_u64("DST_ITERATIONS", 5_000);

    // One RNG drives the scenario; a second SeqGen is the IdGen port. Both are
    // seeded so the whole run is reproducible from `seed`.
    let mut rng = SeqGen(seed);
    let mut ids = SeqGen(seed ^ 0xD1CE);

    let capacity = u32::try_from(rng.next_u64() % 50)
        .unwrap_or(0)
        .saturating_add(1);
    let ttl = i64::try_from(rng.next_u64() % 30).unwrap_or(0);
    let mut r = Reservation::new(capacity, ttl);
    let mut clock = SimClock(0);
    let mut live_ids: Vec<u64> = Vec::new();

    for step in 0..iterations {
        match rng.next_u64() % 4 {
            0 => {
                let seats = u32::try_from(rng.next_u64() % 10).unwrap_or(0);
                if let Ok(h) = r.hold(seats, &clock, &mut ids) {
                    live_ids.push(h.id);
                }
            }
            1 => {
                if let Some(&id) = pick(&live_ids, &mut rng) {
                    let _ = r.confirm(id, &clock);
                }
            }
            2 => {
                if let Some(&id) = pick(&live_ids, &mut rng) {
                    let _ = r.release(id, &clock);
                }
            }
            _ => {
                let delta = i64::try_from(rng.next_u64() % 15).unwrap_or(0);
                clock.0 = clock.0.saturating_add(delta);
            }
        }

        // The safety property: confirmed + currently-held never exceeds
        // capacity, and never overflows. A violation here means overbooking.
        let total = r.confirmed().checked_add(r.held(clock.0));
        assert!(
            matches!(total, Some(t) if t <= capacity),
            "OVERBOOKING at step {step} (seed={seed}): confirmed={}, held={}, capacity={capacity}\n  \
             reproduce: DST_SEED={seed} DST_ITERATIONS={iterations} cargo test -p api --test dst",
            r.confirmed(),
            r.held(clock.0),
        );
    }

    println!("DST OK: seed={seed} iterations={iterations} capacity={capacity} ttl={ttl}");
}

/// Pick a pseudo-random element of `xs` (or `None` if empty), advancing `rng`.
fn pick<'a>(xs: &'a [u64], rng: &mut SeqGen) -> Option<&'a u64> {
    if xs.is_empty() {
        return None;
    }
    let idx = usize::try_from(rng.next_u64()).unwrap_or(0) % xs.len();
    xs.get(idx)
}
