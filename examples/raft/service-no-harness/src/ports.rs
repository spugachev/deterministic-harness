//! Determinism ports: all time and randomness flow through these traits so an
//! entire cluster run is reproducible from a single seed.

/// Logical clock. Time is measured in abstract "ticks" advanced by the
/// simulator, never read from the wall clock.
pub trait Clock {
    fn now(&self) -> u64;
}

/// Deterministic randomness source for election-timeout jitter and ids.
pub trait Rng {
    /// Returns a value in `[low, high)`. `low < high` is required by callers.
    fn gen_range(&mut self, low: u64, high: u64) -> u64;
}

/// A tiny SplitMix64 generator — deterministic, seedable, dependency-free.
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl Rng for SplitMix64 {
    fn gen_range(&mut self, low: u64, high: u64) -> u64 {
        debug_assert!(low < high);
        let span = high - low;
        low + (self.next_u64() % span)
    }
}
