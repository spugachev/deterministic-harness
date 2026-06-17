//! Ports — the determinism seam. ALL non-determinism (wall-clock, randomness,
//! id generation) flows through these traits. Production wires real adapters;
//! tests/DST wire deterministic ones with a fixed seed. This is what makes the
//! DST/Loom/TSAN gates meaningful and keeps the domain reproducible.
//!
//! Rule (enforced by `clippy.toml` `disallowed_methods`): domain/application
//! code must NEVER call `SystemTime::now`, `Instant::now`, or a thread RNG
//! directly — go through a port.

/// A source of the current time (seconds since the Unix epoch).
pub trait Clock {
    /// Current time as whole seconds since the Unix epoch.
    fn now_unix(&self) -> i64;
}

/// A source of randomness.
pub trait Rng {
    /// Next pseudo-random `u64`.
    fn next_u64(&mut self) -> u64;
}

/// A source of fresh, unique identifiers.
pub trait IdGen {
    /// A new identifier, monotonically increasing within a run.
    fn next_id(&mut self) -> u64;
}

/// A deterministic clock fixed at construction — the DST/test adapter.
#[derive(Clone, Copy, Debug)]
pub struct FixedClock(pub i64);

impl Clock for FixedClock {
    fn now_unix(&self) -> i64 {
        self.0
    }
}

/// A tiny deterministic counter implementing both [`Rng`] and [`IdGen`] — seed
/// it for reproducible test/DST runs.
#[derive(Clone, Copy, Debug)]
pub struct SeqGen(pub u64);

impl Rng for SeqGen {
    fn next_u64(&mut self) -> u64 {
        // SplitMix64 step — deterministic from the seed.
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl IdGen for SeqGen {
    fn next_id(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1);
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_is_constant() {
        let c = FixedClock(1_700_000_000);
        assert_eq!(c.now_unix(), 1_700_000_000);
        assert_eq!(c.now_unix(), c.now_unix());
    }

    #[test]
    fn seqgen_is_deterministic_from_seed() {
        let mut a = SeqGen(42);
        let mut b = SeqGen(42);
        assert_eq!(a.next_u64(), b.next_u64());
        assert_eq!(a.next_id(), b.next_id());
    }
}
