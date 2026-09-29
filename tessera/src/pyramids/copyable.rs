//! Copyable: whether a same-size tile holds the same cells as another
//! -- what a copy reads, and what each child of a masking copy reads.
//!
//! A copy names one of four offsets, near or far ([`CopyOffsets`], by
//! default [`NEAR_OFFSETS`] and [`FAR_OFFSETS`]): the same-size tile that
//! many tiles away holds its cells. The offsets are part of the format:
//! a stream decodes only with the offsets it was encoded with. Each child of a masking copy reads its same child of that
//! tile: the offset, counted in child sides -- twice as many.
//!
//! Two tiles hold the same cells exactly when their pattern numbers
//! ([`super::patterns`]) are equal: every match is one comparison.

use super::patterns::Patterns;
use crate::tile::{directions, Tile, CELL_LEVEL, CHILDREN_ACROSS, DIRECTIONS};

/// Where a near copy reads from, by direction, in tiles of its own size:
/// its neighbours before it in reading order -- top left, above, top
/// right, left.
pub const NEAR_OFFSETS: [(isize, isize); 4] = DIRECTIONS;

/// Where a far copy reads from, by direction, in tiles of its own size:
/// top left two tiles away, above and left four, top right four across
/// and four up -- found by the diagnostics tool's copy offset search,
/// where the near offsets doubled lost several percent on cities and
/// more on checkerboards and the saved adversarial bitmaps. Every climb
/// of that search, from the offsets before and from random ones, near
/// and far together, reached these eight positions.
pub const FAR_OFFSETS: [(isize, isize); 4] = [(-2, -2), (0, -4), (4, -4), (-4, 0)];

/// Whether an offset reads a tile before the copy in reading order --
/// above, or left in the same row -- as every offset must, so decoding
/// resolves every copy, each source before what copies it.
pub const fn precedes((dx, dy): (isize, isize)) -> bool {
    dy < 0 || (dy == 0 && dx < 0)
}

/// The default offsets all precede.
const _: () = {
    let mut direction = 0;
    while direction < NEAR_OFFSETS.len() {
        assert!(precedes(NEAR_OFFSETS[direction]) && precedes(FAR_OFFSETS[direction]));
        direction += 1;
    }
};

/// The offsets copies read from, near and far, by direction, in tiles of
/// the copy's own size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CopyOffsets {
    /// A near copy's, by direction.
    near: [(isize, isize); 4],
    /// A far copy's, by direction.
    far: [(isize, isize); 4],
}

impl Default for CopyOffsets {
    /// [`NEAR_OFFSETS`] and [`FAR_OFFSETS`].
    fn default() -> Self {
        Self { near: NEAR_OFFSETS, far: FAR_OFFSETS }
    }
}

impl CopyOffsets {
    /// Copies reading from `near` and `far`, by direction, if every one
    /// [`precedes`] the copy and no two are the same tile.
    pub fn new(near: [(isize, isize); 4], far: [(isize, isize); 4]) -> Option<Self> {
        let mut all = [(0, 0); 2 * DIRECTIONS.len()];
        let (all_near, all_far) = all.split_at_mut(near.len());
        all_near.copy_from_slice(&near);
        all_far.copy_from_slice(&far);
        let distinct = all.iter().enumerate().all(|(position, offset)| !all[..position].contains(offset));
        (distinct && all.iter().all(|&offset| precedes(offset))).then_some(Self { near, far })
    }

    /// The near offsets.
    pub fn near(&self) -> [(isize, isize); 4] {
        self.near
    }

    /// The far offsets.
    pub fn far(&self) -> [(isize, isize); 4] {
        self.far
    }

    /// The offset a near or far copy in `direction` reads from.
    #[inline]
    pub fn offset(&self, far: bool, direction: u8) -> (isize, isize) {
        if far { self.far[direction as usize] } else { self.near[direction as usize] }
    }
}

/// A copy's `offset`, counted in its children's sides: where each child
/// reads its same child of the source.
pub fn child_offset((dx, dy): (isize, isize)) -> (isize, isize) {
    let across = CHILDREN_ACROSS as isize;
    (dx * across, dy * across)
}

/// Nothing finer than 4x4 copies: the tree holds no node finer than the
/// 4x4 floor, so a copy of a 2x2 would never reach the stream.
pub const FINEST_COPY_LEVEL: u8 = CELL_LEVEL - 2;

/// The first direction whose near or far copy of `tile`, by `offsets`,
/// holds the same cells, if any: none past the edge.
pub fn matching_direction(patterns: &Patterns, offsets: &CopyOffsets, tile: Tile, far: bool) -> Option<u8> {
    let mine = patterns.number(tile);
    if !patterns.repeats(tile.level, mine) {
        return None;
    }
    directions().find(|&direction| matches_at(patterns, tile, mine, offsets.offset(far, direction)))
}

/// Whether the same-size tile `offset` away from `tile` holds the same
/// cells as `tile`, whose pattern number is `mine`.
#[inline]
pub fn matches_at(patterns: &Patterns, tile: Tile, mine: u16, offset: (isize, isize)) -> bool {
    tile.offset_by(offset).is_some_and(|other| patterns.number(other) == mine)
}

/// There are this many directions, one bit each in a set of them.
const _: () = assert!(DIRECTIONS.len() <= u8::BITS as usize);
