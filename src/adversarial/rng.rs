//! A small, seeded random source: SplitMix64, whose whole state is one
//! word, so a search is settled by its seed alone.

/// SplitMix64's constants, as published with it: the step added to
/// the state each draw...
const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
/// ...the first mixing multiplier...
const MIX_1: u64 = 0xBF58_476D_1CE4_E5B9;
/// ...and the second.
const MIX_2: u64 = 0x94D0_49BB_1331_11EB;

/// The random source.
pub struct Rng(
    /// Its whole state: the seed, before the first draw.
    u64,
);

impl Rng {
    /// A source settled by `seed`.
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next draw: any 64-bit value.
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(GOLDEN_GAMMA);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(MIX_1);
        z = (z ^ (z >> 27)).wrapping_mul(MIX_2);
        z ^ (z >> 31)
    }

    /// A number in `0..bound`.
    pub fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    /// A number in `low..=high`.
    pub fn between(&mut self, low: u64, high: u64) -> u64 {
        low + self.below(high - low + 1)
    }

    /// A number in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next() >> (u64::BITS - f64::MANTISSA_DIGITS)) as f64 / (1u64 << f64::MANTISSA_DIGITS) as f64
    }
}
