//! Copyable: whether a same-size tile holds the same cells as another
//! -- what a copy reads, and what each child of a masking copy reads:
//!
//! - near: a neighbour of the tile itself, what a near copy reads;
//! - far: the same tile two tiles away -- a neighbour of the tile's
//!   parent, at the tile's own child position -- what a far copy reads,
//!   and what each child of a near copy reads;
//! - four tiles away, what each child of a far copy reads.
//!
//! Two tiles hold the same cells exactly when their pattern numbers
//! ([`super::patterns`]) are equal: every match is one comparison.

use super::patterns::Patterns;
use crate::gct::tile::{directions, Tile, CELL_LEVEL, DIRECTIONS};

/// How many tiles away a near copy reads from: its own neighbour.
pub const NEAR_DISTANCE: usize = 1;
/// How many tiles away a far copy reads from: its parent's neighbour,
/// at the tile's own child position.
pub const FAR_DISTANCE: usize = 2;

/// Nothing finer than 4x4 copies. A 2x2 is either homogeneous, a tile,
/// or its four cells are the residual pass's own -- a copy there would
/// never reach the stream. A cell is always homogeneous.
pub const FINEST_COPY_LEVEL: u8 = CELL_LEVEL - 2;

/// The first direction, in [`DIRECTIONS`], whose same-size tile
/// `distance` away from `tile` holds the same cells, if any: none past
/// the edge.
pub fn matching_direction(patterns: &Patterns, tile: Tile, distance: usize) -> Option<u8> {
    let mine = patterns.number(tile);
    directions().find(|&direction| matches_at(patterns, tile, mine, direction, distance))
}

/// Whether the same-size tile `distance` away from `tile` in
/// `direction` holds the same cells as `tile`, whose pattern number is
/// `mine`.
#[inline]
pub fn matches_at(patterns: &Patterns, tile: Tile, mine: u16, direction: u8, distance: usize) -> bool {
    tile.neighbour_at(direction, distance).is_some_and(|other| patterns.number(other) == mine)
}

/// There are this many directions, one bit each in a set of them.
const _: () = assert!(DIRECTIONS.len() <= u8::BITS as usize);
