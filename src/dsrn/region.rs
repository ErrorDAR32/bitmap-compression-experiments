//! What a region is: a level and a place in that level's tile plane.
//!
//! A region carries no cell coordinates. Its level says both how big
//! it is and which plane its `x` and `y` are counted in, because a
//! region of level `L` and a tile of level `L` are the same square --
//! same size, same alignment, same grid. Cell coordinates are worked
//! out once, at the bottom, where raw bits are actually read.
//!
//! It is one file because it is one thing. A region is small enough
//! that what it is and what can be asked of it read as one idea, and
//! splitting them would cost a folder and a hop to say the same.

use crate::pyramid::{tile_of_bitmap, tile_side, tiles_across, Pyramid, CELL_LEVEL};
use crate::Bitmap;

/// A square of the quadtree, which is also a tile of its own level.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Region {
    /// 0 is the whole bitmap, [`CELL_LEVEL`] is one cell.
    pub level: usize,
    /// Where it sits in its level's tile plane.
    pub x: usize,
    pub y: usize,
}

/// The four children of a region, in reading order: top left, top
/// right, bottom left, bottom right.
pub const CHILDREN: [(usize, usize); 4] = [(0, 0), (1, 0), (0, 1), (1, 1)];

/// How many children a region has.
pub const CHILD_COUNT: usize = CHILDREN.len();

/// A mask naming every child.
pub const EVERY_CHILD: u64 = 0b1111;

/// Where a region may copy from: the four neighbours of its own size
/// that reading order puts before it.
pub const DIRECTIONS: [(isize, isize); 4] = [(-1, -1), (0, -1), (1, -1), (-1, 0)];

impl Region {
    /// The whole bitmap.
    pub const fn whole_bitmap() -> Self {
        Self { level: 0, x: 0, y: 0 }
    }

    /// Whether the region is a single cell, which is as far down as
    /// anything goes.
    pub const fn is_a_cell(self) -> bool {
        self.level == CELL_LEVEL
    }

    /// Its side, in cells.
    pub const fn side_in_cells(self) -> usize {
        tile_side(self.level)
    }

    /// Where its top left cell is. The one place a region turns into
    /// cell coordinates.
    pub const fn top_left_cell(self) -> (usize, usize) {
        let side = self.side_in_cells();
        (self.x * side, self.y * side)
    }

    /// Its four children, in reading order.
    pub fn children(self) -> [Region; CHILD_COUNT] {
        CHILDREN.map(|(dx, dy)| Region {
            level: self.level + 1,
            x: self.x * 2 + dx,
            y: self.y * 2 + dy,
        })
    }

    /// The neighbour in a direction, if it is on the bitmap.
    pub fn neighbour(self, direction: usize) -> Option<Region> {
        let (dx, dy) = DIRECTIONS[direction];
        let (x, y) = (self.x as isize + dx, self.y as isize + dy);
        let across = tiles_across(self.level) as isize;
        (x >= 0 && y >= 0 && x < across && y < across).then_some(Region {
            level: self.level,
            x: x as usize,
            y: y as usize,
        })
    }

    /// The tiles that fill this region at `depth` levels below it, in
    /// reading order.
    pub fn tiles_at_depth(self, depth: usize) -> Vec<Region> {
        let across = 1usize << depth;
        let level = self.level + depth;
        let mut out = Vec::with_capacity(across * across);
        for row in 0..across {
            for col in 0..across {
                out.push(Region { level, x: self.x * across + col, y: self.y * across + row });
            }
        }
        out
    }

    /// Which of this region's children a tile of it falls in.
    pub fn child_holding(self, depth: usize, tile: Region) -> usize {
        let half = 1usize << (depth - 1);
        let across = 1usize << depth;
        let (col, row) = (tile.x - self.x * across, tile.y - self.y * across);
        (row >= half) as usize * 2 + (col >= half) as usize
    }
}

/// How many tiles fill a region `depth` levels below it.
pub const fn tiles_at_depth(depth: usize) -> usize {
    1 << (2 * depth)
}

/// The depths a region may name, coarsest first: from itself as one
/// tile down to its cells.
pub const fn deepest_depth(level: usize) -> usize {
    CELL_LEVEL - level
}


/// Whether two regions of the same size hold the same cells.
pub fn same_cells(bitmap: &Bitmap, a: Region, b: Region) -> bool {
    let ((ax, ay), (bx, by)) = (a.top_left_cell(), b.top_left_cell());
    let side = a.side_in_cells();
    for row in 0..side {
        for col in 0..side {
            if bitmap.get((ax + col) as u8, (ay + row) as u8)
                != bitmap.get((bx + col) as u8, (by + row) as u8)
            {
                return false;
            }
        }
    }
    true
}

/// Whether every cell of a region is clear, which is what a region
/// left alone by a subdivision stays.
pub fn all_cells_clear(pyramid: &Pyramid, bitmap: &Bitmap, region: Region) -> bool {
    tile_of_bitmap(pyramid, bitmap, region.level, region.x, region.y)
        == Some(false)
}

/// Whether every cell of a region has been encoded already, and so
/// will be there for the decoder to copy from.
pub fn whole_region_encoded(encoded_cells: &Bitmap, region: Region) -> bool {
    let (x, y) = region.top_left_cell();
    let side = region.side_in_cells();
    for row in 0..side {
        for col in 0..side {
            if !encoded_cells.get((x + col) as u8, (y + row) as u8) {
                return false;
            }
        }
    }
    true
}
