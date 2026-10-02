//! How many of a bitmap's cells are set before each of its words, in
//! Morton order: counted once a bitmap, then read by everything that
//! needs a set count of a run of whole words -- the binary count tree, a half
//! at a time, and a cell list's tile, 8x8 or coarser -- as one
//! subtraction.
//!
//! Function by function: `docs/reference.md`, "`set_cells_before_each_word.rs`".

use crate::tile::{cells_in_tile, Tile};
use bitmap::{Bitmap, WORDS};

/// Cells a word of the bitmap holds.
const WORD_CELLS: usize = u64::BITS as usize;

/// Cells set before each word, and in all.
pub struct SetCellsBeforeEachWord {
    /// Cells set before word `i`, at `i`; in all, at the last.
    set_before: Box<[u32; WORDS + 1]>,
}

impl SetCellsBeforeEachWord {
    /// Room for a bitmap's counts, none counted yet.
    pub fn new() -> Self {
        Self { set_before: Box::new([0; WORDS + 1]) }
    }

    /// `bitmap`'s counts, whatever these held before.
    pub fn count(&mut self, bitmap: &Bitmap) {
        for (index, word) in bitmap.words().iter().enumerate() {
            self.set_before[index + 1] = self.set_before[index] + word.count_ones();
        }
    }

    /// Cells set in all.
    pub fn total(&self) -> u64 {
        self.set_before[WORDS] as u64
    }

    /// Cells set in the run of `count` words from `first`.
    pub fn in_words(&self, first: usize, count: usize) -> u64 {
        (self.set_before[first + count] - self.set_before[first]) as u64
    }

    /// Cells set in `tile`, a word or more of cells: one run of whole
    /// words.
    pub fn in_tile(&self, tile: Tile) -> u64 {
        let cells = cells_in_tile(tile.level);
        debug_assert!(cells >= WORD_CELLS, "{tile:?} is less than a word of cells");
        self.in_words(tile.first_cell() / WORD_CELLS, cells / WORD_CELLS)
    }
}
