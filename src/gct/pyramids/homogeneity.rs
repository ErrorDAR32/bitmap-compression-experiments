//! The homogeneity pyramid: for every tile, down to single cells,
//! whether all of its cells agree, and on what. Two bits a tile.
//!
//! Built by setting every cell; each set propagates upward, since a tile
//! is homogeneous exactly when its four children are homogeneous and
//! agree.

use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{Tile, CELL_LEVEL};
use crate::Bitmap;

const HOMOGENEOUS: u64 = 0b01;
const VALUE: u64 = 0b10;

const SHAPE: PyramidShape = PyramidShape { arity: 4, coarsest_level: 0, finest_level: CELL_LEVEL, element_bits: 2 };

pub trait Homogeneity {
    /// The homogeneity pyramid of `bitmap`.
    fn homogeneity(bitmap: &Bitmap) -> Self;

    /// What `tile` holds, if every cell of it agrees.
    fn homogeneous_value(&self, tile: Tile) -> Option<bool>;
}

impl Homogeneity for Pyramid {
    fn homogeneity(bitmap: &Bitmap) -> Self {
        let mut pyramid = Pyramid::with_propagation(SHAPE, all_children_homogeneous_and_agreeing);
        for cell in pyramid.tiles_of_level(CELL_LEVEL).collect::<Vec<_>>() {
            let value = if cell.top_left_value(bitmap) { VALUE } else { 0 };
            pyramid.set(cell, HOMOGENEOUS | value);
        }
        pyramid
    }

    fn homogeneous_value(&self, tile: Tile) -> Option<bool> {
        let element = self.get(tile);
        (element & HOMOGENEOUS != 0).then_some(element & VALUE != 0)
    }
}

/// The propagation: a tile is homogeneous, holding its children's
/// shared value, exactly when all of its children are homogeneous and
/// hold the same value.
fn all_children_homogeneous_and_agreeing(pyramid: &Pyramid, tile: Tile) -> u64 {
    let children: Vec<u64> = pyramid.children_of(tile).into_iter().map(|child| pyramid.get(child)).collect();
    let first = children[0];
    if children.iter().all(|&child| child & HOMOGENEOUS != 0 && child == first) {
        first
    } else {
        0
    }
}
