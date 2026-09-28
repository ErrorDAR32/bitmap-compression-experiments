//! Copyable: whether a same-size tile holds the same cells as another
//! -- what a copy reads, and what each child of a masking copy reads.
//! Asked only of the tiles the greedy tiler reaches and cannot bind,
//! so answered on demand, not held for every tile:
//!
//! - near: a neighbour of the tile itself, what a near copy reads;
//! - far: the same tile two tiles away -- a neighbour of the tile's
//!   parent, at the tile's own child position -- what a far copy reads,
//!   and what each child of a near copy reads;
//! - four tiles away, what each child of a far copy reads.
//!
//! Two homogeneous tiles hold the same cells exactly when their values
//! agree, and a homogeneous tile never holds what a non-homogeneous one
//! does, so only two non-homogeneous tiles are compared cell run against
//! cell run -- one compare each, in Morton order.

use super::homogeneity::Homogeneity;
use super::pyramid::Pyramid;
use crate::gct::tile::{directions, same_cells, Tile, CELL_LEVEL, DIRECTIONS};
use crate::Bitmap;

/// How many tiles away a near copy reads from: its own neighbour.
pub const NEAR_DISTANCE: usize = 1;
/// How many tiles away a far copy reads from: its parent's neighbour,
/// at the tile's own child position.
pub const FAR_DISTANCE: usize = 2;

/// Nothing finer than 4x4 copies. A 2x2 is either homogeneous, a tile,
/// or its four cells are the residual pass's own -- a copy there would
/// never reach the stream. A cell is always homogeneous.
pub const FINEST_COPY_LEVEL: u8 = CELL_LEVEL - 2;

/// Every direction, in [`DIRECTIONS`], whose same-size tile `distance`
/// away from `tile` holds the same cells, bit `d` for direction `d`:
/// none past the edge. `homogeneity` is `bitmap`'s.
pub fn matching_directions(homogeneity: &Pyramid, bitmap: &Bitmap, tile: Tile, distance: usize) -> u8 {
    let mine = homogeneity.homogeneous_value(tile);
    let mut matching = 0;
    for direction in directions() {
        if matches_at(homogeneity, bitmap, tile, mine, direction, distance) {
            matching |= 1 << direction;
        }
    }
    matching
}

/// The first direction whose tile `distance` away holds the same cells
/// as `tile`, if any.
pub fn matching_direction(homogeneity: &Pyramid, bitmap: &Bitmap, tile: Tile, distance: usize) -> Option<u8> {
    let mine = homogeneity.homogeneous_value(tile);
    directions().find(|&direction| matches_at(homogeneity, bitmap, tile, mine, direction, distance))
}

/// Whether the same-size tile `distance` away from `tile` in
/// `direction` holds the same cells as `tile`, whose homogeneous value,
/// if any, is `mine` -- looked up once for every direction asked.
#[inline]
fn matches_at(homogeneity: &Pyramid, bitmap: &Bitmap, tile: Tile, mine: Option<bool>, direction: u8, distance: usize) -> bool {
    let Some(other) = tile.neighbour_at(direction, distance) else { return false };
    match (mine, homogeneity.homogeneous_value(other)) {
        (None, None) => same_cells(bitmap, tile, other),
        (mine, theirs) => mine == theirs,
    }
}

/// There are this many directions, one bit each in a set of them.
const _: () = assert!(DIRECTIONS.len() <= u8::BITS as usize);
