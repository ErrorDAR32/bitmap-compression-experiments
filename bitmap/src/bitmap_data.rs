//! The bitmap itself: 256 by 256 bits, packed into machine words.
//!
//! In [Morton order](crate::morton): the cell at Morton index `i` is bit
//! `i % 64` of word `i / 64`. Every tile -- an aligned square whose side
//! is a power of two -- is then one contiguous run of bits: a 4x4 tile
//! sixteen bits, an 8x8 tile exactly one word, a bigger one whole words.
//! So a question about a whole tile is a few word operations, not one
//! per row.
//!
//! The drawing methods (`bitmap_drawing.rs`) take `i64` and clamp, so a
//! caller can ask for a circle hanging off the edge without doing the
//! arithmetic first.

use crate::morton::morton_index;
use crate::{BITS_PER_WORD, WORDS};

/// Every cell of a bitmap, 64 a word, in Morton order.
pub type CellWords = [u64; WORDS];

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
    pub fn words(&self) -> &CellWords {
        &self.words
    }

    /// The cells, 64 a word, in Morton order, to write.
    pub fn words_mut(&mut self) -> &mut CellWords {
        &mut self.words
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

    /// Whether no cell is set: stops at the first word with one.
    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&word| word == 0)
    }

    /// Sets the cells of a tile smaller than a word -- top left at
    /// `(x, y)`, `side` cells a side -- whose bits are set in `run`, in
    /// Morton order; its other cells stay as they are.
    pub fn set_in_small_tile(&mut self, (x, y): (u8, u8), side: usize, run: u64) {
        self.set_in_morton_run(morton_index(x, y), side * side, run);
    }

    /// `cells` cells, fewer than a word, from the one at Morton index
    /// `first_cell_index`, a multiple of `cells`: one run, which never
    /// straddles two words.
    pub fn morton_run(&self, first_cell_index: usize, cells: usize) -> u64 {
        (self.words[first_cell_index / BITS_PER_WORD] >> (first_cell_index % BITS_PER_WORD)) & low_bits_mask(cells)
    }

    /// Sets the cells, of the `cells` from Morton index
    /// `first_cell_index` (fewer than a word, the index a multiple of
    /// it), whose bits are set in `run`; the others stay as they are.
    pub fn set_in_morton_run(&mut self, first_cell_index: usize, cells: usize, run: u64) {
        debug_assert!(run & !low_bits_mask(cells) == 0, "{run:#b} is more than {cells} cells");
        self.words[first_cell_index / BITS_PER_WORD] |= run << (first_cell_index % BITS_PER_WORD);
    }

    /// Makes every cell what it is in `other`, in place.
    pub fn copy_from(&mut self, other: &Bitmap) {
        *self.words = *other.words;
    }

    /// Clears the `cells` from Morton index `first_cell_index` (fewer
    /// than a word, the index a multiple of it).
    pub fn clear_morton_run(&mut self, first_cell_index: usize, cells: usize) {
        self.words[first_cell_index / BITS_PER_WORD] &= !(low_bits_mask(cells) << (first_cell_index % BITS_PER_WORD));
    }

    /// The words of a tile of a word or more -- top left at `(x, y)`,
    /// `side` cells a side -- to write, in Morton order.
    pub fn tile_words_mut(&mut self, (x, y): (u8, u8), side: usize) -> &mut [u64] {
        let (first_cell_index, cells) = (morton_index(x, y), side * side);
        debug_assert!(cells >= BITS_PER_WORD, "a {side}x{side} tile is less than a word");
        &mut self.words[first_cell_index / BITS_PER_WORD..(first_cell_index + cells) / BITS_PER_WORD]
    }

    /// A tile's cells, a word at a time in Morton order: its words, or,
    /// for a tile of fewer than 64 cells, its one run.
    pub fn tile_words(&self, (x, y): (u8, u8), side: usize) -> impl Iterator<Item = u64> + '_ {
        let (first_cell_index, cells) = (morton_index(x, y), side * side);
        let (whole_words, small_run) = if cells >= BITS_PER_WORD {
            (&self.words[first_cell_index / BITS_PER_WORD..(first_cell_index + cells) / BITS_PER_WORD], None)
        } else {
            (&self.words[..0], Some(self.morton_run(first_cell_index, cells)))
        };
        whole_words.iter().copied().chain(small_run)
    }

    /// The set cells of a tile, each as its place in the tile's own
    /// Morton order, in that order.
    pub fn set_cells_in_tile(&self, corner: (u8, u8), side: usize) -> impl Iterator<Item = usize> + '_ {
        self.tile_words(corner, side).enumerate().flat_map(|(word_index, word)| {
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

    /// Sets the cell at `place`, in the tile's own Morton order, of the
    /// tile whose top left cell is `(x, y)`.
    pub fn set_in_tile(&mut self, (x, y): (u8, u8), place: usize) {
        let cell_index = morton_index(x, y) + place;
        self.words[cell_index / BITS_PER_WORD] |= 1u64 << (cell_index % BITS_PER_WORD);
    }

    /// Sets every cell of a tile.
    pub fn set_tile(&mut self, (x, y): (u8, u8), side: usize) {
        self.fill_morton_run(morton_index(x, y), side * side);
    }

    /// Sets every one of the `cells` cells from Morton index
    /// `first_cell_index`: an aligned run, `cells` a power of two and the
    /// index a multiple of it -- a tile, or two side by side.
    fn fill_morton_run(&mut self, first_cell_index: usize, cells: usize) {
        if cells >= BITS_PER_WORD {
            self.words[first_cell_index / BITS_PER_WORD..(first_cell_index + cells) / BITS_PER_WORD].fill(u64::MAX);
        } else {
            self.words[first_cell_index / BITS_PER_WORD] |= low_bits_mask(cells) << (first_cell_index % BITS_PER_WORD);
        }
    }
}

impl Default for Bitmap {
    /// The same as [`Bitmap::new`]: empty.
    fn default() -> Self {
        Self::new()
    }
}
