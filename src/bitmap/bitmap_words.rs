//! Word-level helpers for reading a line of the matrix.
//!
//! Every structure in the crate that holds one bit per position holds
//! it the same way: four `u64` to a line of 256, least significant bit
//! first, so position `p` is bit `p % 64` of word `p / 64`. This is an
//! operation that shape needs: a range is never walked, only masked.

/// The bits of word `index` that lie in `lo..=hi`, as a mask.
///
/// Words wholly outside the range come back empty, so a caller can walk
/// all four words of a line and let the mask decide which ones matter,
/// rather than working out which words the range touches first.
///
/// ```text
///   range_mask(0,  4, 9)  ->  0b...0011_1111_0000   bits 4 to 9
///   range_mask(0, 70, 80) ->  0                     range is past word 0
///   range_mask(1, 70, 80) ->  bits 6 to 16          of word 1
/// ```
pub(crate) fn range_mask(index: usize, lo: u8, hi: u8) -> u64 {
    let base = index * 64;
    let lo = (lo as usize).max(base);
    let hi = (hi as usize).min(base + 63);
    if lo > hi {
        return 0;
    }
    (u64::MAX << (lo - base)) & (u64::MAX >> (base + 63 - hi))
}

