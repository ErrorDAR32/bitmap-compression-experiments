//! Word-level helpers for reading a line of the matrix.
//!
//! Every structure in the crate that holds one bit per position holds
//! it the same way: four `u64` to a line of 256, least significant bit
//! first, so position `p` is bit `p % 64` of word `p / 64`. These are
//! the operations that shape needs, and they are here rather than in
//! any one of the modules that use them because three of them do.
//!
//! All of them are a handful of instructions whatever the line holds. A
//! run is never searched for, only read off the bits around a position,
//! and a range is never walked, only masked.

/// How many machine words hold one line of the matrix.
pub(crate) const LINE_WORDS: usize = 256 / 64;

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

/// The position of the next set bit at or after `from`, if any.
///
/// Walks word by word rather than bit by bit, so an empty stretch of
/// 192 positions costs three loads and three tests.
pub(crate) fn next_set(words: &[u64], from: usize) -> Option<usize> {
    let mut index = from / 64;
    // Everything below `from` in its own word is not a candidate.
    let mut word = words[index] & (u64::MAX << (from % 64));
    loop {
        if word != 0 {
            return Some(index * 64 + word.trailing_zeros() as usize);
        }
        index += 1;
        word = *words.get(index)?;
    }
}

/// The position of the next clear bit at or after `from`, or one past
/// the last position when the line is set all the way to its end.
///
/// The end is `words.len() * 64` rather than `None`, because every
/// caller wants "where the run ends" and a run that reaches the edge of
/// the matrix ends there.
pub(crate) fn next_clear(words: &[u64], from: usize) -> usize {
    let mut index = from / 64;
    let mut word = !words[index] & (u64::MAX << (from % 64));
    loop {
        if word != 0 {
            return index * 64 + word.trailing_zeros() as usize;
        }
        index += 1;
        match words.get(index) {
            Some(&next) => word = !next,
            None => return words.len() * 64,
        }
    }
}

/// The position of the last clear bit strictly before `from`, or -1
/// when the line is set all the way back to its start.
///
/// Signed, for the same reason [`next_clear`] runs past the end: the
/// caller wants "where the run begins", which is one past this, and a
/// run beginning at position 0 has to be expressible.
pub(crate) fn prev_clear(words: &[u64], from: usize) -> i32 {
    let mut index = from / 64;
    let bit = from % 64;
    // Everything at or above `from` in its own word is not a candidate.
    // A shift by 64 is undefined, so bit 0 masks the word away instead.
    let mut word = !words[index] & (u64::MAX >> (64 - bit)) & if bit == 0 { 0 } else { u64::MAX };
    loop {
        if word != 0 {
            return (index * 64) as i32 + 63 - word.leading_zeros() as i32;
        }
        if index == 0 {
            return -1;
        }
        index -= 1;
        word = !words[index];
    }
}
