//! What a pyramid is: two bit planes, one level to a plane.
//!
//! # Levels
//!
//! A **level** counts down in size from the whole bitmap. Level 0 is
//! one tile of 256 by 256; level 8 is 65536 tiles of one cell. So a
//! level is both a tile size and a coordinate system: at level `L`
//! there are `1 << L` tiles across, and a tile's own side is
//! `256 >> L`.
//!
//! A region of the quadtree and a tile of the same level are the same
//! square. That is not a coincidence to be kept in mind; it is the
//! whole reason the encoding works, and it means nothing here needs a
//! cell coordinate. A tile is a level and a place in that level's
//! plane, and so is a region.
//!
//! # The two planes
//!
//! For every tile the pyramid holds two bits: whether the tile is
//! homogeneous, and, where it is, what it holds. They are kept as
//! separate planes rather than interleaved so that a level of either
//! can be read a machine word at a time.
//!
//! The cells are not held. Level 8 is the bitmap itself, and asking
//! the pyramid about a cell would be storing the bitmap twice.

use crate::WIDTH;

/// The level of a single cell, and so the finest level there is.
pub const CELL_LEVEL: usize = 8;

/// The finest level the pyramid holds. The cells are the bitmap.
pub const FINEST_LEVEL_HELD: usize = CELL_LEVEL - 1;

/// Tiles across one row of a level's plane.
pub const fn tiles_across(level: usize) -> usize {
    1 << level
}

/// The side of a tile at a level, in cells.
pub const fn tile_side(level: usize) -> usize {
    WIDTH >> level
}

/// Tiles in a whole level's plane.
pub const fn tiles_in_level(level: usize) -> usize {
    tiles_across(level) * tiles_across(level)
}

/// Machine words a level's plane takes. Even level 0, one tile, takes
/// a whole word.
const fn words_in_level(level: usize) -> usize {
    let bits = tiles_in_level(level);
    if bits < 64 {
        1
    } else {
        bits / 64
    }
}

/// Where each level's plane starts, and where the last one ends.
///
/// A plane is laid out row by row, so a tile at `(x, y)` of level `L`
/// is bit `y * tiles_across(L) + x` of the run beginning at
/// `PYRAMID_LEVEL_BOUNDARIES[L]`.
pub const PYRAMID_LEVEL_BOUNDARIES: [usize; FINEST_LEVEL_HELD + 2] = {
    let mut at = [0; FINEST_LEVEL_HELD + 2];
    let mut level = 0;
    while level <= FINEST_LEVEL_HELD {
        at[level + 1] = at[level] + words_in_level(level);
        level += 1;
    }
    at
};

/// Every word of one plane.
pub const WORDS_IN_A_PLANE: usize = PYRAMID_LEVEL_BOUNDARIES[FINEST_LEVEL_HELD + 1];

/// Whether each tile is homogeneous, and what the homogeneous ones
/// hold.
///
/// Held inline rather than boxed: a pyramid is built once and read in
/// place, never moved, and the two planes together are under five
/// kilobytes.
pub struct Pyramid {
    /// A bit per tile, set where every cell of the tile agrees.
    pub(crate) homogeneous_tiles: [u64; WORDS_IN_A_PLANE],
    /// What a homogeneous tile holds. Meaningless where the tile is
    /// not homogeneous, which no reader looks at.
    pub(crate) homogeneous_tile_values: [u64; WORDS_IN_A_PLANE],
}

impl Default for Pyramid {
    fn default() -> Self {
        Self::new()
    }
}

impl Pyramid {
    /// An empty pyramid, with its room already found.
    pub fn new() -> Self {
        Self {
            homogeneous_tiles: [0; WORDS_IN_A_PLANE],
            homogeneous_tile_values: [0; WORDS_IN_A_PLANE],
        }
    }

    /// Clears every level, so one pyramid can take the next bitmap.
    pub fn clear(&mut self) {
        self.homogeneous_tiles.fill(0);
        self.homogeneous_tile_values.fill(0);
    }

    /// Where a tile's bit sits in a plane.
    pub(crate) fn bit_of_tile(level: usize, x: usize, y: usize) -> (usize, usize) {
        let bit = y * tiles_across(level) + x;
        (PYRAMID_LEVEL_BOUNDARIES[level] + bit / 64, bit % 64)
    }

    /// What the tile at `(x, y)` of `level` holds, if every cell of it
    /// agrees. The cells are not held here -- ask the bitmap.
    pub fn tile(&self, level: usize, x: usize, y: usize) -> Option<bool> {
        let (word, shift) = Self::bit_of_tile(level, x, y);
        if self.homogeneous_tiles[word] >> shift & 1 == 0 {
            return None;
        }
        Some(self.homogeneous_tile_values[word] >> shift & 1 != 0)
    }
}
