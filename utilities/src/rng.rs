//! TileSim's one random source: SplitMix64, seeded, whose whole state
//! is one word, so whatever draws from it -- a test bitmap, a search --
//! is settled by its seed alone, on every run and every machine.

use crate::hash::{mix, GOLDEN_RATIO, MIX_1};

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

    /// A source of its own for `stream` -- a superchunk index, say -- of
    /// `seed`: its state is mixed, not `seed` moved along, so
    /// two streams are not one sequence a few draws apart, as two
    /// sources whose seeds differ by a multiple of the step would be.
    pub fn for_stream(seed: u64, stream: u64) -> Self {
        let mut mixer = Self(seed ^ stream.wrapping_mul(MIX_1));
        let first = mixer.draw();
        Self(first ^ mixer.draw().rotate_left(32))
    }

    /// Its whole state: a generator made from it draws what this one
    /// would have. What a save keeps.
    pub fn state(&self) -> u64 {
        self.0
    }

    /// The next draw: any 64-bit value.
    pub fn draw(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(GOLDEN_RATIO);
        mix(self.0)
    }

    /// A number in `0..bound`.
    pub fn below(&mut self, bound: u64) -> u64 {
        self.draw() % bound
    }

    /// A number in `low..=high`.
    pub fn between(&mut self, low: u64, high: u64) -> u64 {
        low + self.below(high - low + 1)
    }

    /// True `percent` times in a hundred.
    pub fn percent_chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    /// A number in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.draw() >> (u64::BITS - f64::MANTISSA_DIGITS)) as f64 / (1u64 << f64::MANTISSA_DIGITS) as f64
    }
}
