//! Homogeneity, bits 0-1 of the [content pyramid](super::content): for
//! every tile, down to single cells, whether all of its cells agree,
//! and on what.
//!
//! Filled a word at a time, the way a mipmap is built: the cells straight
//! off the bitmap's words, then each level from the one finer, since a
//! tile is homogeneous exactly when its four children are homogeneous
//! and agree. Both are laid out in Morton order, so a tile's four
//! children are four consecutive elements: one word, its own element is
//! folded from.

use super::content::SHAPE;
use super::pyramid::Pyramid;
use crate::gct::tile::{Tile, CELL_LEVEL};
use crate::Bitmap;

/// Bit 0: whether every cell of the tile agrees.
const HOMOGENEOUS: u64 = 0b01;
/// Bit 1: the value they agree on, when they do.
const VALUE: u64 = 0b10;

/// Homogeneity's query, over the content pyramid.
pub trait Homogeneity {
    /// What `tile` holds, if every cell of it agrees.
    fn homogeneous_value(&self, tile: Tile) -> Option<bool>;
}

impl Homogeneity for Pyramid {
    fn homogeneous_value(&self, tile: Tile) -> Option<bool> {
        let element = self.get(tile);
        (element & HOMOGENEOUS != 0).then_some(element & VALUE != 0)
    }
}

/// Elements a word.
const PER_WORD: usize = u64::BITS as usize / SHAPE.element_bits;
/// Four elements' bit 0, and their bit 1: the homogeneous and value
/// bits of a tile's four children, which are one word.
const CHILDREN_HOMOGENEOUS: u64 = 0x0001_0001_0001_0001 * HOMOGENEOUS;
/// The four children's value bits, in the same word.
const CHILDREN_VALUES: u64 = 0x0001_0001_0001_0001 * VALUE;
/// Every run of cells a word of elements holds, homogeneous, each cell's
/// value its element's value bit: one entry for every value of that many
/// cells.
const CELLS: [u64; 1 << PER_WORD] = {
    let mut table = [0; 1 << PER_WORD];
    let mut cells = 0;
    while cells < table.len() {
        let mut cell = 0;
        while cell < PER_WORD {
            let value = if cells >> cell & 1 != 0 { VALUE } else { 0 };
            table[cells] |= (HOMOGENEOUS | value) << (cell * SHAPE.element_bits);
            cell += 1;
        }
        cells += 1;
    }
    table
};

/// Fills bits 0-1 of every element of a fresh content pyramid.
pub(super) fn fill_homogeneity(pyramid: &mut Pyramid, bitmap: &Bitmap) {
    let cells = pyramid.level_words_mut(CELL_LEVEL);
    for (at, &word) in bitmap.words().iter().enumerate() {
        for part in 0..u64::BITS as usize / PER_WORD {
            let run = (word >> (part * PER_WORD)) as usize & ((1 << PER_WORD) - 1);
            cells[at * u64::BITS as usize / PER_WORD + part] = CELLS[run];
        }
    }
    for level in (0..CELL_LEVEL).rev() {
        let (coarser, finer) = pyramid.two_levels_mut(level);
        for (at, &children) in finer.iter().enumerate() {
            coarser[at / PER_WORD] |= folded(children) << (at % PER_WORD * SHAPE.element_bits);
        }
    }
}

/// A tile's element from its four children's, one word: homogeneous,
/// holding their value, exactly when all four are homogeneous and hold
/// the same one.
fn folded(children: u64) -> u64 {
    let values = children & CHILDREN_VALUES;
    let homogeneous = children & CHILDREN_HOMOGENEOUS == CHILDREN_HOMOGENEOUS
        && (values == 0 || values == CHILDREN_VALUES);
    if homogeneous {
        children & (HOMOGENEOUS | VALUE)
    } else {
        0
    }
}
