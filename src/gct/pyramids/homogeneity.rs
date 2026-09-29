//! The homogeneity pyramid: for every tile, down to single cells,
//! whether all of its cells agree, and on what. Two bits a tile.
//!
//! Built a word at a time, the way a mipmap is: the cells straight off
//! the bitmap's words, then each level from the one finer, since a tile
//! is homogeneous exactly when its four children are homogeneous and
//! agree. Both are laid out in Morton order, so a tile's four children
//! are the four consecutive elements -- one byte -- its own element is
//! folded from.

use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{Tile, CELL_LEVEL, CHILDREN};
use crate::Bitmap;

/// Bit 0: whether every cell of the tile agrees.
const HOMOGENEOUS: u64 = 0b01;
/// Bit 1: the value they agree on, when they do.
const VALUE: u64 = 0b10;

/// Two bits a tile, every level down to single cells.
const SHAPE: PyramidShape = PyramidShape { coarsest_level: 0, finest_level: CELL_LEVEL, element_bits: 2 };

/// Building the homogeneity pyramid, and its query.
pub trait Homogeneity {
    /// The homogeneity pyramid of `bitmap`.
    fn homogeneity(bitmap: &Bitmap) -> Self;

    /// Makes this homogeneity pyramid `bitmap`'s, in place: every
    /// element is written, so nothing needs clearing first.
    fn rebuild_homogeneity(&mut self, bitmap: &Bitmap);

    /// What `tile` holds, if every cell of it agrees.
    fn homogeneous_value(&self, tile: Tile) -> Option<bool>;

    /// What each of `tile`'s four children holds, in reading order, if
    /// every cell of it agrees: one lookup, their four elements being
    /// one byte.
    fn children_values(&self, tile: Tile) -> [Option<bool>; 4];
}

impl Homogeneity for Pyramid {
    fn homogeneity(bitmap: &Bitmap) -> Self {
        let mut pyramid = Pyramid::new(SHAPE);
        pyramid.rebuild_homogeneity(bitmap);
        pyramid
    }

    fn rebuild_homogeneity(&mut self, bitmap: &Bitmap) {
        // Every cell is homogeneous: two words of elements to a word of
        // cells, each cell's value its element's value bit.
        let cells = self.level_words_mut(CELL_LEVEL);
        for (cell_word_index, &cell_word) in bitmap.words().iter().enumerate() {
            cells[2 * cell_word_index] = every_cell(cell_word as u32);
            cells[2 * cell_word_index + 1] = every_cell((cell_word >> u32::BITS) as u32);
        }
        for level in (0..CELL_LEVEL).rev() {
            let (coarser, finer) = self.two_levels_mut(level);
            for (coarser_word_index, finer_words) in finer.chunks(FINER_WORDS_A_WORD).enumerate() {
                coarser[coarser_word_index] = finer_words
                    .iter()
                    .enumerate()
                    .map(|(quarter, &finer_word)| folded(finer_word) << (quarter as u32 * FOLDED_BITS))
                    .fold(0, |coarser_word, quarter_bits| coarser_word | quarter_bits);
            }
        }
    }

    fn homogeneous_value(&self, tile: Tile) -> Option<bool> {
        value_of(self.get(tile))
    }

    #[inline]
    fn children_values(&self, tile: Tile) -> [Option<bool>; 4] {
        self.children_elements(tile).map(value_of)
    }
}

/// What a tile whose element is `element` holds, if every cell of it
/// agrees.
#[inline]
fn value_of(element: u64) -> Option<bool> {
    (element & HOMOGENEOUS != 0).then_some(element & VALUE != 0)
}

/// Every even bit: each element's homogeneous bit, word-wide.
const EVEN_BITS: u64 = 0x5555_5555_5555_5555;
/// The lowest bit of every byte: each coarser tile's, once its four
/// children's byte is folded.
const BYTE_LOW_BITS: u64 = 0x0101_0101_0101_0101;
/// A word of finer elements folds to this many coarser elements' bits:
/// four children a coarser tile, so a quarter of a word.
const FOLDED_BITS: u32 = u64::BITS / CHILDREN as u32;
/// So this many words of a finer level fold into one of a coarser.
const FINER_WORDS_A_WORD: usize = CHILDREN as usize;

/// Thirty-two cells as homogeneous elements: each value spread to the
/// odd bits, every homogeneous bit set.
fn every_cell(values: u32) -> u64 {
    let mut spread = values as u64;
    spread = (spread | spread << 16) & 0x0000_ffff_0000_ffff;
    spread = (spread | spread << 8) & 0x00ff_00ff_00ff_00ff;
    spread = (spread | spread << 4) & 0x0f0f_0f0f_0f0f_0f0f;
    spread = (spread | spread << 2) & 0x3333_3333_3333_3333;
    spread = (spread | spread << 1) & EVEN_BITS;
    spread << 1 | EVEN_BITS
}

/// A word of 32 elements folded into their 8 parents' elements, in the
/// word's low quarter: a parent is homogeneous, holding its children's
/// value, exactly when all four are homogeneous and hold the same one.
fn folded(children: u64) -> u64 {
    let all_four = |bits: u64| bits & bits >> 2 & bits >> 4 & bits >> 6 & BYTE_LOW_BITS;
    let (homogeneous, values) = (children & EVEN_BITS, children >> 1 & EVEN_BITS);
    let all_set = all_four(values);
    let all_clear = all_four(!values & EVEN_BITS);
    let homogeneous_parents = all_four(homogeneous) & (all_set | all_clear);
    // Each parent's two bits at the bottom of its byte, then the bytes
    // packed together.
    let mut packed = homogeneous_parents | (all_set & homogeneous_parents) << 1;
    packed = (packed | packed >> 6) & 0x000f_000f_000f_000f;
    packed = (packed | packed >> 12) & 0x0000_00ff_0000_00ff;
    (packed | packed >> 24) & 0xffff
}
