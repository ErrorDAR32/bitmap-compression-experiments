//! A superchunk's heights: one [`Height`] a cell, held raw, 8 to a
//! word -- the words a superchunk image holds them in.
//!
//! The heights are laid out in Morton order over the whole superchunk:
//! chunk by chunk in their Morton order, and in each chunk in the Morton
//! order its bitmaps use (`bitmap::morton`). So each chunk's heights
//! are one run, and any aligned square of cells is one run of heights,
//! as it is one run of bits in a layer.

use coordinates::{CellPlace, ChunkPlace, CHUNK_SIDE, CHUNKS_IN_SUPERCHUNK};
use bitmap::morton::morton_index;

/// A cell's height.
pub type Height = u8;

/// Heights a word holds.
const HEIGHTS_IN_WORD: usize = (u64::BITS / Height::BITS) as usize;

/// Words a superchunk's heights take.
pub const HEIGHT_WORDS: usize = CHUNK_SIDE * CHUNK_SIDE * CHUNKS_IN_SUPERCHUNK / HEIGHTS_IN_WORD;

/// Where the height of `cell` in the chunk at `chunk` is: its word, and
/// the shift to its byte there.
fn word_and_shift(chunk: ChunkPlace, cell: CellPlace) -> (usize, u32) {
    let index = chunk.index() * CHUNK_SIDE * CHUNK_SIDE + morton_index(cell.x, cell.y);
    (index / HEIGHTS_IN_WORD, (index % HEIGHTS_IN_WORD) as u32 * Height::BITS)
}

/// The height of `cell` in the chunk at `chunk`, from a superchunk's
/// height words.
pub fn height_in(words: &[u64], chunk: ChunkPlace, cell: CellPlace) -> Height {
    let (word, shift) = word_and_shift(chunk, cell);
    (words[word] >> shift) as Height
}

/// A superchunk's heights, one a cell, in Morton order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeightMap {
    /// Every cell's height, 8 a word, the first lowest.
    words: Box<[u64]>,
}

impl HeightMap {
    /// Every cell at `height`.
    pub fn filled(height: Height) -> Self {
        Self { words: vec![u64::from_le_bytes([height; HEIGHTS_IN_WORD]); HEIGHT_WORDS].into_boxed_slice() }
    }

    /// The heights in `words`, as a superchunk image holds them.
    pub fn from_words(words: &[u64]) -> Self {
        assert_eq!(words.len(), HEIGHT_WORDS, "a superchunk's heights");
        Self { words: words.into() }
    }

    /// The height of `cell` in the chunk at `chunk`.
    pub fn get(&self, chunk: ChunkPlace, cell: CellPlace) -> Height {
        height_in(&self.words, chunk, cell)
    }

    /// Sets the height of `cell` in the chunk at `chunk`.
    pub fn set(&mut self, chunk: ChunkPlace, cell: CellPlace, height: Height) {
        let (word, shift) = word_and_shift(chunk, cell);
        self.words[word] = self.words[word] & !((Height::MAX as u64) << shift) | (height as u64) << shift;
    }

    /// Every height, 8 a word, in Morton order: what an image holds.
    pub fn words(&self) -> &[u64] {
        &self.words
    }
}

impl Default for HeightMap {
    /// Every cell at height 0.
    fn default() -> Self {
        Self::filled(0)
    }
}
