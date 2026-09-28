//! Copyable: whether a same-size tile holds the same cells as another
//! -- what a copy reads, and what each child of a masking copy reads.
//!
//! A copy names one of four offsets, near or far ([`NEAR_OFFSETS`],
//! [`FAR_OFFSETS`]): the same-size tile that many tiles away holds its
//! cells. Each child of a masking copy reads its same child of that
//! tile: the offset, counted in child sides -- twice as many.
//!
//! Two tiles hold the same cells exactly when their pattern numbers
//! ([`super::patterns`]) are equal: every match is one comparison.

use super::patterns::Patterns;
use crate::gct::tile::{directions, Tile, CELL_LEVEL, CHILDREN_ACROSS, DIRECTIONS};

/// Where a near copy reads from, by direction, in tiles of its own size:
/// its neighbours before it in reading order -- top left, above, top
/// right, left.
pub const NEAR_OFFSETS: [(isize, isize); 4] = DIRECTIONS;

/// Where a far copy reads from, by direction, in tiles of its own size:
/// the near offsets, twice as far.
pub const FAR_OFFSETS: [(isize, isize); 4] = [(-2, -2), (0, -2), (2, -2), (-2, 0)];

/// Every offset reads a tile before the copy in reading order -- above,
/// or left in the same row -- so decoding resolves every copy, each
/// source before what copies it.
const _: () = {
    let mut at = 0;
    while at < NEAR_OFFSETS.len() {
        let ((near_x, near_y), (far_x, far_y)) = (NEAR_OFFSETS[at], FAR_OFFSETS[at]);
        assert!(near_y < 0 || (near_y == 0 && near_x < 0));
        assert!(far_y < 0 || (far_y == 0 && far_x < 0));
        at += 1;
    }
};

/// The offset, in tiles of its own size, a copy reads from.
pub fn copy_offset(far: bool, direction: u8) -> (isize, isize) {
    if far { FAR_OFFSETS[direction as usize] } else { NEAR_OFFSETS[direction as usize] }
}

/// A copy's `offset`, counted in its children's sides: where each child
/// reads its same child of the source.
pub fn child_offset((dx, dy): (isize, isize)) -> (isize, isize) {
    let across = CHILDREN_ACROSS as isize;
    (dx * across, dy * across)
}

/// Nothing finer than 4x4 copies. A 2x2 is either homogeneous, a tile,
/// or its four cells are the residual pass's own -- a copy there would
/// never reach the stream. A cell is always homogeneous.
pub const FINEST_COPY_LEVEL: u8 = CELL_LEVEL - 2;

/// The first direction whose near or far copy of `tile` holds the same
/// cells, if any: none past the edge.
pub fn matching_direction(patterns: &Patterns, tile: Tile, far: bool) -> Option<u8> {
    let mine = patterns.number(tile);
    directions().find(|&direction| matches_at(patterns, tile, mine, copy_offset(far, direction)))
}

/// Whether the same-size tile `offset` away from `tile` holds the same
/// cells as `tile`, whose pattern number is `mine`.
#[inline]
pub fn matches_at(patterns: &Patterns, tile: Tile, mine: u16, offset: (isize, isize)) -> bool {
    tile.offset_by(offset).is_some_and(|other| patterns.number(other) == mine)
}

/// There are this many directions, one bit each in a set of them.
const _: () = assert!(DIRECTIONS.len() <= u8::BITS as usize);
