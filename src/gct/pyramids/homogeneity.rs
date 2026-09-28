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
use crate::gct::tile::{Tile, CELL_LEVEL};
use crate::Bitmap;

const HOMOGENEOUS: u64 = 0b01;
const VALUE: u64 = 0b10;

const SHAPE: PyramidShape = PyramidShape { coarsest_level: 0, finest_level: CELL_LEVEL, element_bits: 2 };

pub trait Homogeneity {
    /// The homogeneity pyramid of `bitmap`.
    fn homogeneity(bitmap: &Bitmap) -> Self;

    /// What `tile` holds, if every cell of it agrees.
    fn homogeneous_value(&self, tile: Tile) -> Option<bool>;
}

impl Homogeneity for Pyramid {
    fn homogeneity(bitmap: &Bitmap) -> Self {
        let mut pyramid = Pyramid::new(SHAPE);
        // Every cell is homogeneous: two words of elements to a word of
        // cells, each cell's value its element's value bit.
        let cells = pyramid.level_words_mut(CELL_LEVEL);
        for (at, &word) in bitmap.words().iter().enumerate() {
            cells[2 * at] = every_cell(word as u32);
            cells[2 * at + 1] = every_cell((word >> u32::BITS) as u32);
        }
        for level in (0..CELL_LEVEL).rev() {
            let (coarser, finer) = pyramid.two_levels_mut(level);
            for (at, children) in finer.chunks(FINER_WORDS_A_WORD).enumerate() {
                coarser[at] = children
                    .iter()
                    .enumerate()
                    .map(|(i, &word)| folded(word) << (i as u32 * FOLDED_BITS))
                    .fold(0, |word, part| word | part);
            }
        }
        pyramid
    }

    fn homogeneous_value(&self, tile: Tile) -> Option<bool> {
        let element = self.get(tile);
        (element & HOMOGENEOUS != 0).then_some(element & VALUE != 0)
    }
}

/// Every even bit: each element's homogeneous bit, word-wide.
const EVEN_BITS: u64 = 0x5555_5555_5555_5555;
/// The lowest bit of every byte: each coarser tile's, once its four
/// children's byte is folded.
const BYTE_LOW_BITS: u64 = 0x0101_0101_0101_0101;
/// A word of finer elements folds to this many coarser elements' bits:
/// four children a coarser tile, so a quarter of a word.
const FOLDED_BITS: u32 = u64::BITS / 4;
/// So this many words of a finer level fold into one of a coarser.
const FINER_WORDS_A_WORD: usize = 4;

/// Thirty-two cells as homogeneous elements: each value spread to the
/// odd bits, every homogeneous bit set.
fn every_cell(values: u32) -> u64 {
    let mut v = values as u64;
    v = (v | v << 16) & 0x0000_ffff_0000_ffff;
    v = (v | v << 8) & 0x00ff_00ff_00ff_00ff;
    v = (v | v << 4) & 0x0f0f_0f0f_0f0f_0f0f;
    v = (v | v << 2) & 0x3333_3333_3333_3333;
    v = (v | v << 1) & EVEN_BITS;
    v << 1 | EVEN_BITS
}

/// A word of 32 elements folded into their 8 parents' elements, in the
/// word's low quarter: a parent is homogeneous, holding its children's
/// value, exactly when all four are homogeneous and hold the same one.
fn folded(children: u64) -> u64 {
    let all_four = |bits: u64| bits & bits >> 2 & bits >> 4 & bits >> 6 & BYTE_LOW_BITS;
    let (homogeneous, values) = (children & EVEN_BITS, children >> 1 & EVEN_BITS);
    let ones = all_four(values);
    let zeros = all_four(!values & EVEN_BITS);
    let parents = all_four(homogeneous) & (ones | zeros);
    // Each parent's two bits at the bottom of its byte, then the bytes
    // packed together.
    let mut packed = parents | (ones & parents) << 1;
    packed = (packed | packed >> 6) & 0x000f_000f_000f_000f;
    packed = (packed | packed >> 12) & 0x0000_00ff_0000_00ff;
    (packed | packed >> 24) & 0xffff
}
