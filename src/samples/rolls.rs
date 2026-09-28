//! The seeded dice the drawn samples are rolled with: a plain linear
//! congruential generator, so a seed means a bitmap and nothing drifts
//! between runs or machines.

/// Knuth's MMIX multiplier...
const MULTIPLIER: u64 = 6364136223846793005;
/// ...and increment.
const INCREMENT: u64 = 1442695040888963407;
/// The low bits of an LCG repeat quickly; only the high ones are used.
const DISCARDED_LOW_BITS: u32 = 33;

/// The dice.
pub(super) struct Rolls(
    /// The generator's state: the seed, before the first roll.
    pub(super) u64,
);

impl Rolls {
    /// The next roll: the state's high bits, after one step.
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
        self.0 >> DISCARDED_LOW_BITS
    }

    /// A number in `0..high`.
    pub(super) fn upto(&mut self, high: u64) -> u64 {
        self.next() % high
    }

    /// True `in_a_hundred` times in a hundred.
    pub(super) fn chance(&mut self, in_a_hundred: u64) -> bool {
        self.upto(100) < in_a_hundred
    }
}
