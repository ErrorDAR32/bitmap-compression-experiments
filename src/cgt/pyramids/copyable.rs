//! The copyable pyramid: for every tile coarser than a cell, whether
//! it could copy, two bits a tile --
//!
//! - near: some same-size neighbour of the tile itself, in
//!   [`DIRECTIONS`], holds the same cells;
//! - far: some same-size neighbour of the tile's *parent*, at the child
//!   position the tile occupies within it, does -- the same neighbour
//!   test one parent width (two tile widths) away.
//!
//! It says whether asking which direction is worth it at all, not which
//! direction: the question is asked of nearly every tile and the answer
//! is nearly always no. Nothing propagates here -- a tile matching its
//! neighbour says nothing about whether their parents match -- so each
//! level is read off the cells, a row of a tile at a time.

use super::pyramid::{Pyramid, PyramidShape};
use crate::cgt::tile::{tile_side, tiles_across, Tile, CELL_LEVEL, DIRECTIONS};
use crate::Bitmap;

const NEAR: u64 = 0b01;
const FAR: u64 = 0b10;

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
            let (across, side) = (tiles_across(level) as isize, tile_side(level));
            for y in 0..across {
                for x in 0..across {
                    let matches_at = |distance: isize| {
                        DIRECTIONS.iter().any(|&(dx, dy)| {
                            let (at_x, at_y) = (x + distance * dx, y + distance * dy);
                            at_x >= 0
                                && at_y >= 0
                                && at_x < across
                                && at_y < across
                                && same_tiles(bitmap, side, (x as usize, y as usize), (at_x as usize, at_y as usize))
                        })
                    };
                    let near = if matches_at(1) { NEAR } else { 0 };
                    let far = if matches_at(2) { FAR } else { 0 };
                    pyramid.set(Tile { level, x: x as usize, y: y as usize }, near | far);
                }
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

/// Whether two same-size tiles hold the same cells, a word of a row at
/// a time. A tile's row is a run of `side` bits starting at a multiple
/// of `side`, so it is either whole words or a run inside one word.
fn same_tiles(bitmap: &Bitmap, side: usize, a: (usize, usize), b: (usize, usize)) -> bool {
    let (a_x, b_x) = (a.0 * side, b.0 * side);
    (0..side).all(|row| {
        let mine = bitmap.row((a.1 * side + row) as u8);
        let theirs = bitmap.row((b.1 * side + row) as u8);
        if side >= 64 {
            let words = side / 64;
            return (0..words).all(|word| mine[a_x / 64 + word] == theirs[b_x / 64 + word]);
        }
        let mask = (1u64 << side) - 1;
        (mine[a_x / 64] >> (a_x % 64)) & mask == (theirs[b_x / 64] >> (b_x % 64)) & mask
    })
}
