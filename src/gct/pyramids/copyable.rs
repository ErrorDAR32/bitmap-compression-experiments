//! The copyable pyramid: for every tile coarser than a cell, whether
//! it could copy, two bits a tile --
//!
//! - near: some same-size neighbour of the tile itself, in
//!   [`DIRECTIONS`](crate::gct::tile::DIRECTIONS), holds the same cells;
//! - far: some same-size neighbour of the tile's *parent*, at the child
//!   position the tile occupies within it, does -- the same tile two
//!   tiles away.
//!
//! It tells whether asking which direction is worth it at all, not which
//! direction: the question is asked of nearly every tile and the answer
//! is nearly always no. Nothing propagates here -- a tile matching its
//! neighbour tells nothing about whether their parents match -- so each
//! level is read off the cells.

use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{directions, same_cells, Tile, CELL_LEVEL};
use crate::Bitmap;

const NEAR: u64 = 0b01;
const FAR: u64 = 0b10;

/// How many tiles away a near copy and a far copy read from.
pub const NEAR_DISTANCE: usize = 1;
pub const FAR_DISTANCE: usize = 2;

/// Cells never copy: a single cell is always homogeneous, so the
/// greedy tiler binds it before ever asking.
const SHAPE: PyramidShape = PyramidShape { arity: 4, coarsest_level: 0, finest_level: CELL_LEVEL - 1, element_bits: 2 };

pub trait Copyable {
    /// The copyable pyramid of `bitmap`.
    fn copyable(bitmap: &Bitmap) -> Self;

    /// Whether some same-size neighbour of `tile` holds the same cells.
    fn near_copyable(&self, tile: Tile) -> bool;

    /// Whether some same-size neighbour of `tile`'s parent, at `tile`'s
    /// own child position, holds the same cells.
    fn far_copyable(&self, tile: Tile) -> bool;
}

impl Copyable for Pyramid {
    fn copyable(bitmap: &Bitmap) -> Self {
        let mut pyramid = Pyramid::new(SHAPE);
        for level in 0..=SHAPE.finest_level {
            for tile in pyramid.tiles_of_level(level).collect::<Vec<_>>() {
                let matches_at = |distance: usize| {
                    directions().any(|direction| {
                        tile.neighbour_at(direction, distance).is_some_and(|other| same_cells(bitmap, tile, other))
                    })
                };
                let near = if matches_at(NEAR_DISTANCE) { NEAR } else { 0 };
                let far = if matches_at(FAR_DISTANCE) { FAR } else { 0 };
                pyramid.set(tile, near | far);
            }
        }
        pyramid
    }

    fn near_copyable(&self, tile: Tile) -> bool {
        self.get(tile) & NEAR != 0
    }

    fn far_copyable(&self, tile: Tile) -> bool {
        self.get(tile) & FAR != 0
    }
}
