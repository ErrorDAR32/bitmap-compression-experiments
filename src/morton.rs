//! Morton (Z) order: a square plane's cells numbered by interleaving the
//! bits of their coordinates, `x` in the even bits and `y` in the odd
//! ones. The first sixteen run
//!
//! ```text
//!  0  1  4  5
//!  2  3  6  7
//!  8  9 12 13
//! 10 11 14 15
//! ```
//!
//! so every aligned square of a power-of-two side is one contiguous run,
//! and a square's four quarters are four consecutive runs. The bitmap
//! and every level of every pyramid are laid out this way.

/// Every byte with its bits spread to every other bit: bit `i` to bit
/// `2i`. A table, since this is asked of every coordinate read.
const SPREAD: [u16; 256] = {
    let mut table = [0u16; 256];
    let mut v = 0;
    while v < 256 {
        let mut spread = v;
        spread = (spread | spread << 4) & 0x0f0f;
        spread = (spread | spread << 2) & 0x3333;
        spread = (spread | spread << 1) & 0x5555;
        table[v] = spread as u16;
        v += 1;
    }
    table
};

/// The Morton index of `(x, y)`.
pub(crate) fn morton_index(x: u8, y: u8) -> usize {
    SPREAD[x as usize] as usize | (SPREAD[y as usize] as usize) << 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_the_first_sixteen_as_drawn() {
        const FIRST_SIXTEEN: [[usize; 4]; 4] = [[0, 1, 4, 5], [2, 3, 6, 7], [8, 9, 12, 13], [10, 11, 14, 15]];
        for (y, row) in FIRST_SIXTEEN.iter().enumerate() {
            for (x, &index) in row.iter().enumerate() {
                assert_eq!(morton_index(x as u8, y as u8), index);
            }
        }
        assert_eq!(morton_index(u8::MAX, u8::MAX), u16::MAX as usize);
    }
}
