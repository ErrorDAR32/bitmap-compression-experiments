//! The bitmap itself: 256 by 256 bits, packed into machine words.
//!
//! In [Morton order](crate::morton): the cell at Morton index `i` is bit
//! `i % 64` of word `i / 64`. Every aligned square of a power-of-two side
//! -- every tile -- is then one contiguous run of bits: a 4x4 sixteen
//! bits, an 8x8 exactly one word, anything bigger whole words. So
//! whole-square questions are a few word operations, not one a row.
//!
//! The drawing methods (`bitmap_drawing.rs`) take `i64` and clamp, so a
//! caller can ask for a circle hanging off the edge without doing the
//! arithmetic first.

use crate::morton::morton_index;
use crate::{BITS_PER_WORD, WORDS};

/// Every cell of a bitmap, 64 a word, in Morton order.
pub(crate) type CellWords = [u64; WORDS];

/// 65536 bits, boxed so that passing one around moves a pointer rather
/// than eight kilobytes.
#[derive(Clone)]
pub struct Bitmap {
    /// The cells, 64 a word, in Morton order.
    words: Box<CellWords>,
}

/// The low `cells` bits set, fewer than a word.
fn low_bits_mask(cells: usize) -> u64 {
    (1u64 << cells) - 1
}

impl Bitmap {
    /// A bitmap with nothing set.
    pub fn new() -> Self {
        Self { words: Box::new([0u64; WORDS]) }
    }

    /// The cells, 64 a word, in Morton order.
    pub(crate) fn words(&self) -> &CellWords {
        &self.words
    }

    /// The cell at `(x, y)`. Both are `u8`, so every value is a cell of
    /// the 256x256 bitmap: out of bounds cannot be expressed.
    pub fn get(&self, x: u8, y: u8) -> bool {
        let cell_index = morton_index(x, y);
        (self.words[cell_index / BITS_PER_WORD] >> (cell_index % BITS_PER_WORD)) & 1 == 1
    }

    /// Sets the cell at `(x, y)`. Already set is not an error.
    pub fn set(&mut self, x: u8, y: u8) {
        let cell_index = morton_index(x, y);
        self.words[cell_index / BITS_PER_WORD] |= 1u64 << (cell_index % BITS_PER_WORD);
    }

    /// Clears the cell at `(x, y)`. Already clear is not an error.
    pub fn unset(&mut self, x: u8, y: u8) {
        let cell_index = morton_index(x, y);
        self.words[cell_index / BITS_PER_WORD] &= !(1u64 << (cell_index % BITS_PER_WORD));
    }

    /// Clears every cell.
    pub fn reset(&mut self) {
        self.words.fill(0);
    }

    /// How many cells are set, counted a word at a time.
    pub fn count_set(&self) -> u32 {
        self.words.iter().map(|word| word.count_ones()).sum()
    }

    /// An aligned square of fewer than a word of cells, top left at
    /// `(x, y)`, `side` cells a side: its one run, in Morton order.
    pub(crate) fn small_square(&self, (x, y): (u8, u8), side: usize) -> u64 {
        self.morton_run(morton_index(x, y), side * side)
    }

    /// Sets the cells of an aligned square of fewer than a word of cells,
    /// top left at `(x, y)`, `side` cells a side, whose bits are set in
    /// `run`, in Morton order; its other cells stay as they are.
    pub(crate) fn set_in_small_square(&mut self, (x, y): (u8, u8), side: usize, run: u64) {
        self.set_in_morton_run(morton_index(x, y), side * side, run);
    }

    /// `cells` cells, fewer than a word, from the one at Morton index
    /// `first_cell_index`, a multiple of `cells`: one run, which never
    /// straddles two words.
    pub(crate) fn morton_run(&self, first_cell_index: usize, cells: usize) -> u64 {
        (self.words[first_cell_index / BITS_PER_WORD] >> (first_cell_index % BITS_PER_WORD)) & low_bits_mask(cells)
    }

    /// Sets the cells, of the `cells` from Morton index
    /// `first_cell_index` (fewer than a word, the index a multiple of
    /// it), whose bits are set in `run`; the others stay as they are.
    pub(crate) fn set_in_morton_run(&mut self, first_cell_index: usize, cells: usize, run: u64) {
        debug_assert!(run & !low_bits_mask(cells) == 0, "{run:#b} is more than {cells} cells");
        self.words[first_cell_index / BITS_PER_WORD] |= run << (first_cell_index % BITS_PER_WORD);
    }

    /// Makes every cell what it is in `other`, in place.
    pub(crate) fn copy_from(&mut self, other: &Bitmap) {
        *self.words = *other.words;
    }

    /// Clears the `cells` from Morton index `first_cell_index` (fewer
    /// than a word, the index a multiple of it).
    pub(crate) fn clear_morton_run(&mut self, first_cell_index: usize, cells: usize) {
        self.words[first_cell_index / BITS_PER_WORD] &= !(low_bits_mask(cells) << (first_cell_index % BITS_PER_WORD));
    }

    /// The cells of an aligned square of a word of cells or more, top
    /// left at `(x, y)`, `side` cells a side, to write: its words, in
    /// Morton order.
    pub(crate) fn square_words_mut(&mut self, (x, y): (u8, u8), side: usize) -> &mut [u64] {
        let (first_cell_index, cells) = (morton_index(x, y), side * side);
        debug_assert!(cells >= BITS_PER_WORD, "a {side}x{side} square is less than a word");
        &mut self.words[first_cell_index / BITS_PER_WORD..(first_cell_index + cells) / BITS_PER_WORD]
    }

    /// An aligned square's cells, a word at a time in Morton order: its
    /// words, or for a square of fewer than 64 cells, its one run.
    pub(crate) fn square_words(&self, (x, y): (u8, u8), side: usize) -> impl Iterator<Item = u64> + '_ {
        let (first_cell_index, cells) = (morton_index(x, y), side * side);
        let (whole_words, small_run) = if cells >= BITS_PER_WORD {
            (&self.words[first_cell_index / BITS_PER_WORD..(first_cell_index + cells) / BITS_PER_WORD], None)
        } else {
            (&self.words[..0], Some(self.morton_run(first_cell_index, cells)))
        };
        whole_words.iter().copied().chain(small_run)
    }

    /// The set cells of an aligned square, each as its place in the
    /// square's own Morton order, in that order.
    pub(crate) fn set_cells_in_square(&self, corner: (u8, u8), side: usize) -> impl Iterator<Item = usize> + '_ {
        self.square_words(corner, side).enumerate().flat_map(|(word_index, word)| {
            let mut remaining = word;
            std::iter::from_fn(move || {
                (remaining != 0).then(|| {
                    let bit = remaining.trailing_zeros() as usize;
                    remaining &= remaining - 1;
                    word_index * BITS_PER_WORD + bit
                })
            })
        })
    }

    /// Sets the cell at place `place` in the own Morton order of the
    /// aligned square whose top left cell is `(x, y)`.
    pub(crate) fn set_in_square(&mut self, (x, y): (u8, u8), place: usize) {
        let cell_index = morton_index(x, y) + place;
        self.words[cell_index / BITS_PER_WORD] |= 1u64 << (cell_index % BITS_PER_WORD);
    }

    /// Sets every cell of an aligned square.
    pub(crate) fn set_square(&mut self, (x, y): (u8, u8), side: usize) {
        self.set_morton_block(morton_index(x, y), side * side);
    }

    /// Sets every one of the `cells` cells from Morton index
    /// `first_cell_index`: an aligned block, `cells` a power of two and
    /// the index a multiple of it -- a square, or two side by side.
    pub(crate) fn set_morton_block(&mut self, first_cell_index: usize, cells: usize) {
        if cells >= BITS_PER_WORD {
            self.words[first_cell_index / BITS_PER_WORD..(first_cell_index + cells) / BITS_PER_WORD].fill(u64::MAX);
        } else {
            self.words[first_cell_index / BITS_PER_WORD] |= low_bits_mask(cells) << (first_cell_index % BITS_PER_WORD);
        }
    }

    /// How many of the `cells` cells from Morton index
    /// `first_cell_index` are set, an aligned block as for
    /// [`Bitmap::set_morton_block`]: counted a word at a time.
    pub(crate) fn count_in_morton_block(&self, first_cell_index: usize, cells: usize) -> u64 {
        if cells >= BITS_PER_WORD {
            let words = &self.words[first_cell_index / BITS_PER_WORD..(first_cell_index + cells) / BITS_PER_WORD];
            words.iter().map(|word| word.count_ones() as u64).sum()
        } else {
            self.morton_run(first_cell_index, cells).count_ones() as u64
        }
    }
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

    /// Square fills agree with the same squares drawn cell by cell.
    #[test]
    fn squares_agree_with_their_cells() {
        let mut bitmap = Bitmap::new();
        bitmap.set_rect(8, 8, 15, 15);
        bitmap.set_rect(16, 0, 19, 3);
        bitmap.set(24, 4);
        let places: Vec<usize> = bitmap.set_cells_in_square((16, 0), 8).collect();
        assert_eq!(places, (0..16).collect::<Vec<_>>(), "the 4x4 at (16, 0) is the first 16 of its 8x8");
        assert_eq!(bitmap.set_cells_in_square((24, 4), 1).collect::<Vec<_>>(), vec![0]);
        let mut placed = Bitmap::new();
        for place in bitmap.set_cells_in_square((0, 0), 32) {
            placed.set_in_square((0, 0), place);
        }
        assert!((0..32).all(|y| (0..32).all(|x| placed.get(x, y) == bitmap.get(x, y))));
        let mut filled = Bitmap::new();
        filled.set_square((8, 8), 8);
        filled.set_square((16, 0), 4);
        filled.set_square((24, 4), 1);
        assert!((0..=u8::MAX).all(|y| (0..=u8::MAX).all(|x| filled.get(x, y) == bitmap.get(x, y))));
    }

    /// A new bitmap has nothing set.
    #[test]
    fn starts_empty() {
        let bitmap = Bitmap::new();
        assert_eq!(bitmap.count_set(), 0);
        assert!(!bitmap.get(0, 0));
        assert!(!bitmap.get(255, 255));
    }

    /// Setting then unsetting one cell leaves it, and the count, as before.
    #[test]
    fn set_and_unset_single_bit() {
        let mut bitmap = Bitmap::new();
        bitmap.set(10, 20);
        assert!(bitmap.get(10, 20));
        assert_eq!(bitmap.count_set(), 1);
        bitmap.unset(10, 20);
        assert!(!bitmap.get(10, 20));
        assert_eq!(bitmap.count_set(), 0);
    }

    /// A rectangle includes both corners, whichever way round they are
    /// named.
    #[test]
    fn rect_is_inclusive_and_order_independent() {
        let mut bitmap = Bitmap::new();
        bitmap.set_rect(5, 5, 2, 2);
        assert_eq!(bitmap.count_set(), 16);
        assert!((2..=5).all(|y| (2..=5).all(|x| bitmap.get(x, y))));
        bitmap.unset_rect(2, 2, 5, 5);
        assert_eq!(bitmap.count_set(), 0);
    }

    /// A rectangle hanging off the edge is clamped to the bitmap.
    #[test]
    fn rect_clamps_to_bounds() {
        let mut bitmap = Bitmap::new();
        bitmap.set_rect(-10, -10, 1, 1);
        assert_eq!(bitmap.count_set(), 4);
    }

    /// A circle holds its centre and cells at its radius, not its bounding
    /// box's corners.
    #[test]
    fn circle_includes_center_and_excludes_far_corners() {
        let mut bitmap = Bitmap::new();
        bitmap.set_circle(128, 128, 5);
        assert!(bitmap.get(128, 128));
        assert!(bitmap.get(133, 128));
        assert!(!bitmap.get(134, 128));
        assert!(!bitmap.get(133, 133));
    }

    /// Unsetting a circle clears what setting it set.
    #[test]
    fn unset_circle_clears_previously_set_bits() {
        let mut bitmap = Bitmap::new();
        bitmap.set_circle(50, 50, 10);
        assert!(bitmap.count_set() > 0);
        bitmap.unset_circle(50, 50, 10);
        assert_eq!(bitmap.count_set(), 0);
    }

    /// Resetting clears every cell.
    #[test]
    fn reset_clears_everything() {
        let mut bitmap = Bitmap::new();
        bitmap.set_rect(0, 0, 255, 255);
        assert_eq!(bitmap.count_set() as usize, WIDTH * HEIGHT);
        bitmap.reset();
        assert_eq!(bitmap.count_set(), 0);
    }
}
