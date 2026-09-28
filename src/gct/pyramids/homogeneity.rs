//! The homogeneity pyramid: for every tile, down to single cells,
//! whether all of its cells agree, and on what. Two bits a tile.
//!
//! Built by setting every cell, then propagating one action up: a tile
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
        let mut pyramid = Pyramid::new(SHAPE);
        for cell in pyramid.tiles_of_level(CELL_LEVEL).collect::<Vec<_>>() {
            let value = if bitmap.get(cell.x as u8, cell.y as u8) { VALUE } else { 0 };
            pyramid.set(cell, HOMOGENEOUS | value);
        }
        pyramid.propagate(all_homogeneous_and_agreeing);
        pyramid
    }

    fn homogeneous_value(&self, tile: Tile) -> Option<bool> {
        let element = self.get(tile);
        (element & HOMOGENEOUS != 0).then_some(element & VALUE != 0)
    }
}

/// The action: a tile takes its children's shared element when all of
/// them are homogeneous and hold the same value, and is not
/// homogeneous otherwise.
fn all_homogeneous_and_agreeing(children: &[u64]) -> u64 {
    let first = children[0];
    if children.iter().all(|&child| child & HOMOGENEOUS != 0 && child == first) {
        first
    } else {
        0
    }
}
