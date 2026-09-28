//! Matches, bits 2-13 of the [content pyramid](super::content): for
//! every tile down to 4x4, which same-size tiles hold the same cells --
//! one bit for each direction, in
//! [`DIRECTIONS`], at each of
//! [`MATCH_DISTANCES`]:
//!
//! - near: a neighbour of the tile itself, what a near copy reads;
//! - far: the same tile two tiles away -- a neighbour of the tile's
//!   parent, at the tile's own child position -- what a far copy reads,
//!   and what each child of a near copy reads;
//! - four tiles away, what each child of a far copy reads.
//!
//! So which direction a tile copies from, and which children a masking
//! copy says, are read here rather than off the cells. Nothing folds
//! from level to level -- a tile matching its neighbour tells nothing
//! about whether their parents match -- so each level is read off the
//! cells, each tile one run compare in Morton order.

use super::pyramid::Pyramid;
use crate::gct::tile::{directions, same_cells, Tile, CELL_LEVEL, CHILDREN_ACROSS, DIRECTIONS};
use crate::Bitmap;

/// How many tiles away a near copy and a far copy read from.
pub const NEAR_DISTANCE: usize = 1;
pub const FAR_DISTANCE: usize = 2;
/// Every distance a match is held at: a near copy's, a far copy's, and
/// that of a far copy's children.
pub const MATCH_DISTANCES: [usize; 3] = [NEAR_DISTANCE, FAR_DISTANCE, FAR_DISTANCE * CHILDREN_ACROSS as usize];

/// Nothing finer than 4x4 copies. A 2x2 is either homogeneous, a tile,
/// or its four cells are the residual pass's own -- a copy there would
/// never reach the stream. A cell is always homogeneous.
pub const FINEST_COPY_LEVEL: u8 = CELL_LEVEL - 2;

/// The first match bit, after homogeneity's two.
const FIRST_MATCH_BIT: usize = 2;

/// The bit holding whether the tile `distance` away in `direction`
/// holds the same cells.
fn match_bit(direction: u8, distance: usize) -> u64 {
    let at = MATCH_DISTANCES.iter().position(|&held| held == distance).expect("a distance matches are held at");
    1 << (FIRST_MATCH_BIT + at * DIRECTIONS.len() + direction as usize)
}

pub trait Copyable {
    /// Whether the same-size tile `distance` away from `tile` in
    /// `direction` holds the same cells: false past the edge, and for
    /// anything finer than [`FINEST_COPY_LEVEL`].
    fn matches(&self, tile: Tile, direction: u8, distance: usize) -> bool;

    /// The first direction whose tile `distance` away holds the same
    /// cells as `tile`, if any.
    fn matching_direction(&self, tile: Tile, distance: usize) -> Option<u8> {
        directions().find(|&direction| self.matches(tile, direction, distance))
    }
}

impl Copyable for Pyramid {
    fn matches(&self, tile: Tile, direction: u8, distance: usize) -> bool {
        self.get(tile) & match_bit(direction, distance) != 0
    }
}

/// Fills the match bits of a content pyramid.
pub(super) fn fill_matches(pyramid: &mut Pyramid, bitmap: &Bitmap) {
    for level in 0..=FINEST_COPY_LEVEL {
        for tile in Tile::all_of_level(level) {
            let mut matches = 0;
            for distance in MATCH_DISTANCES {
                for direction in directions() {
                    if tile.neighbour_at(direction, distance).is_some_and(|other| same_cells(bitmap, tile, other)) {
                        matches |= match_bit(direction, distance);
                    }
                }
            }
            pyramid.set(tile, pyramid.get(tile) | matches);
        }
    }
}
