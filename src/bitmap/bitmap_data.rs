//! The bitmap itself: 256 by 256 bits, packed into machine words.
//!
//! In [Morton order](crate::morton): cell `i` in that order is bit
//! `i % 64` of word `i / 64`. Every aligned square
//! of a power-of-two side -- every tile -- is then one contiguous run of
//! bits: a 4x4 sixteen bits, an 8x8 exactly one word, anything bigger
//! whole words. So whole-square questions, like [`Bitmap::same_squares`], are a few
//! word operations, not one a row.
//!
//! The drawing methods take `i64` and clamp, so a caller can ask for a
//! circle hanging off the edge without doing the arithmetic first.

use crate::morton::morton_index;
use crate::{BITS_PER_WORD, WORDS};

/// Every cell of a bitmap, 64 a word, in Morton order.
pub(crate) type CellWords = [u64; WORDS];

/// 65536 bits, boxed so that passing one around moves a pointer rather
/// than eight kilobytes.
#[derive(Clone)]
pub struct Bitmap {
    /// The cells, 64 a word, in Morton order.
    pub(crate) words: Box<CellWords>,
}

impl Bitmap {
    /// An empty matrix, with nothing set.
    pub fn new() -> Self {
        Self {
            words: Box::new([0u64; WORDS]),
        }
    }

    /// Where a cell's bit sits: its Morton index. The word is this
    /// divided by 64 and the bit within it the remainder, which is the
    /// whole of the layout.
    fn bit_index(x: u8, y: u8) -> usize {
        morton_index(x, y)
    }

    /// The cells, 64 a word, in Morton order.
    pub(crate) fn words(&self) -> &CellWords {
        &self.words
    }

    /// `x` and `y` are `u8`, so every value from 0 to 255 is a valid
    /// coordinate in this 256x256 matrix and out-of-bounds access is
    /// impossible to express, not just checked at runtime.
    pub fn get(&self, x: u8, y: u8) -> bool {
        let idx = Self::bit_index(x, y);
        (self.words[idx / BITS_PER_WORD] >> (idx % BITS_PER_WORD)) & 1 == 1
    }

    /// Stands the cell up. Already standing is not an error.
    pub fn set(&mut self, x: u8, y: u8) {
        let idx = Self::bit_index(x, y);
        self.words[idx / BITS_PER_WORD] |= 1u64 << (idx % BITS_PER_WORD);
    }

    /// Takes the cell away. Already gone is not an error.
    pub fn unset(&mut self, x: u8, y: u8) {
        let idx = Self::bit_index(x, y);
        self.words[idx / BITS_PER_WORD] &= !(1u64 << (idx % BITS_PER_WORD));
    }

    /// Clears every bit back to 0.
    pub fn reset(&mut self) {
        for word in self.words.iter_mut() {
            *word = 0;
        }
    }

    /// How many cells are standing, counted a word at a time.
    pub fn count_set(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }

    /// Whether two aligned squares of `side` cells, top left at `a` and
    /// at `b`, hold the same cells. Aligned: `side` a power of two, each
    /// corner's coordinates multiples of it.
    pub(crate) fn same_squares(&self, a: (u8, u8), b: (u8, u8), side: usize) -> bool {
        let (a, b) = (Self::bit_index(a.0, a.1), Self::bit_index(b.0, b.1));
        let cells = side * side;
        if cells >= BITS_PER_WORD {
            let (a, b, words) = (a / BITS_PER_WORD, b / BITS_PER_WORD, cells / BITS_PER_WORD);
            return self.words[a..a + words] == self.words[b..b + words];
        }
        self.run(a, cells) == self.run(b, cells)
    }

    /// Sets every cell of an aligned square.
    pub(crate) fn set_square(&mut self, (x, y): (u8, u8), side: usize) {
        let at = Self::bit_index(x, y);
        let cells = side * side;
        if cells >= BITS_PER_WORD {
            self.words[at / BITS_PER_WORD..(at + cells) / BITS_PER_WORD].fill(u64::MAX);
            return;
        }
        self.words[at / BITS_PER_WORD] |= run_mask(cells) << (at % BITS_PER_WORD);
    }

    /// `cells` bits, fewer than a word, from bit `at`: a run never
    /// straddles two words, since it starts at a multiple of its length.
    fn run(&self, at: usize, cells: usize) -> u64 {
        (self.words[at / BITS_PER_WORD] >> (at % BITS_PER_WORD)) & run_mask(cells)
    }
}

/// The low `cells` bits set, fewer than a word.
fn run_mask(cells: usize) -> u64 {
    (1u64 << cells) - 1
}


impl Default for Bitmap {
    /// The same as [`Bitmap::new`]: empty.
    fn default() -> Self {
        Self::new()
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HEIGHT, WIDTH};

    /// Square compares and square fills agree with the same squares drawn
    /// cell by cell.
    #[test]
    fn squares_agree_with_their_cells() {
        let mut m = Bitmap::new();
        m.set_rect(8, 8, 15, 15);
        m.set_rect(16, 0, 19, 3);
        m.set(24, 4);
        assert!(m.same_squares((16, 0), (8, 8), 4));
        assert!(!m.same_squares((16, 0), (24, 4), 4));
        assert!(m.same_squares((0, 64), (64, 0), 64));
        let mut filled = Bitmap::new();
        filled.set_square((8, 8), 8);
        filled.set_square((16, 0), 4);
        filled.set_square((24, 4), 1);
        assert!((0..=u8::MAX).all(|y| (0..=u8::MAX).all(|x| filled.get(x, y) == m.get(x, y))));
    }

    /// A new bitmap has nothing set.
    #[test]
    fn starts_empty() {
        let m = Bitmap::new();
        assert_eq!(m.count_set(), 0);
        assert!(!m.get(0, 0));
        assert!(!m.get(255, 255));
    }

    /// Setting then unsetting one cell leaves it, and the count, as before.
    #[test]
    fn set_and_unset_single_bit() {
        let mut m = Bitmap::new();
        m.set(10, 20);
        assert!(m.get(10, 20));
        assert_eq!(m.count_set(), 1);
        m.unset(10, 20);
        assert!(!m.get(10, 20));
        assert_eq!(m.count_set(), 0);
    }

    /// A rectangle includes both corners, whichever way round they are
    /// named.
    #[test]
    fn rect_is_inclusive_and_order_independent() {
        let mut m = Bitmap::new();
        m.set_rect(5, 5, 2, 2);
        assert_eq!(m.count_set(), 16); // 4x4 inclusive area
        for y in 2..=5 {
            for x in 2..=5 {
                assert!(m.get(x, y));
            }
        }
        m.unset_rect(2, 2, 5, 5);
        assert_eq!(m.count_set(), 0);
    }

    /// A rectangle hanging off the edge is clamped to the bitmap.
    #[test]
    fn rect_clamps_to_bounds() {
        let mut m = Bitmap::new();
        m.set_rect(-10, -10, 1, 1);
        assert_eq!(m.count_set(), 4);
    }

    /// A circle holds its centre and cells at its radius, not its bounding
    /// box's corners.
    #[test]
    fn circle_includes_center_and_excludes_far_corners() {
        let mut m = Bitmap::new();
        m.set_circle(128, 128, 5);
        assert!(m.get(128, 128));
        assert!(m.get(133, 128));
        assert!(!m.get(134, 128));
        // corner of the bounding box should be excluded by the distance test
        assert!(!m.get(133, 133));
    }

    /// Unsetting a circle clears what setting it set.
    #[test]
    fn unset_circle_clears_previously_set_bits() {
        let mut m = Bitmap::new();
        m.set_circle(50, 50, 10);
        let before = m.count_set();
        assert!(before > 0);
        m.unset_circle(50, 50, 10);
        assert_eq!(m.count_set(), 0);
    }

    /// Resetting clears every cell.
    #[test]
    fn reset_clears_everything() {
        let mut m = Bitmap::new();
        m.set_rect(0, 0, 255, 255);
        assert_eq!(m.count_set() as usize, WIDTH * HEIGHT);
        m.reset();
        assert_eq!(m.count_set(), 0);
    }

}
