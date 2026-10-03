//! Hashing, two ways: a key to its slot in a table whose size is a power
//! of two -- Fibonacci hashing, one multiplication -- and a word's bits
//! mixed so every bit of it moves every bit of the result -- SplitMix64's
//! finalizer, what [`crate::rng`] draws with.

/// 2^64 over the golden ratio, odd: multiplying by it spreads a key's
/// low bits into the high ones, which [`slot`] keeps. SplitMix64 steps
/// its state by it too.
pub const GOLDEN_RATIO: u64 = 0x9E37_79B9_7F4A_7C15;
/// SplitMix64's first mixing multiplier...
pub const MIX_1: u64 = 0xBF58_476D_1CE4_E5B9;
/// ...and its second.
pub const MIX_2: u64 = 0x94D0_49BB_1331_11EB;

/// The slot of `key` in a table of `slots` -- a power of two -- by
/// Fibonacci hashing: the top bits of `key` times [`GOLDEN_RATIO`].
#[inline(always)]
pub const fn slot(key: u64, slots: usize) -> usize {
    debug_assert!(slots.is_power_of_two() && slots > 1, "a table of a power of two slots, more than one");
    (key.wrapping_mul(GOLDEN_RATIO) >> (u64::BITS - slots.trailing_zeros())) as usize
}

/// `word`'s bits mixed: SplitMix64's finalizer, each bit of the result
/// moved by every bit of `word`.
#[inline(always)]
pub const fn mix(word: u64) -> u64 {
    let z = (word ^ (word >> 30)).wrapping_mul(MIX_1);
    let z = (z ^ (z >> 27)).wrapping_mul(MIX_2);
    z ^ (z >> 31)
}
