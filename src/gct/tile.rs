//! What a tile is: a size and a place in that size's plane.
//!
//! A tile's level is its size -- level 0 is the whole 256x256 bitmap,
//! level [`CELL_LEVEL`] is one cell -- and its `x` and `y` count tiles
//! of that size, not cells. Cell coordinates are worked out only where
//! cells are actually read.

use crate::{Bitmap, WIDTH};

/// The level of a single cell, the finest there is.
pub const CELL_LEVEL: usize = 8;

/// Where a tile may copy from: the four same-size neighbours reading
/// order puts before it -- top left, above, top right, left.
pub const DIRECTIONS: [(isize, isize); 4] = [(-1, -1), (0, -1), (1, -1), (-1, 0)];

/// Tiles across one row of a level's plane.
pub const fn tiles_across(level: usize) -> usize {
    1 << level
}

/// A tile's side at a level, in cells.
pub const fn tile_side(level: usize) -> usize {
    WIDTH >> level
}

/// How many cells one tile at `level` covers.
pub const fn cells_in_tile(level: usize) -> u64 {
    let side = tile_side(level) as u64;
    side * side
}

/// How many levels lie between a tile at `level` and its cells.
pub const fn levels_to_cells(level: usize) -> usize {
    CELL_LEVEL - level
}

/// A square of the bitmap: its size, and its place among tiles of that
/// size.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tile {
    pub level: usize,
    pub x: usize,
    pub y: usize,
}

impl Tile {
    /// The whole bitmap as one tile.
    pub const fn whole_bitmap() -> Self {
        Self { level: 0, x: 0, y: 0 }
    }

    /// Its side, in cells.
    pub const fn side_in_cells(self) -> usize {
        tile_side(self.level)
    }

    /// Its top left cell.
    pub const fn top_left_cell(self) -> (usize, usize) {
        let side = self.side_in_cells();
        (self.x * side, self.y * side)
    }

    /// Its cells as an inclusive rectangle: left, top, right, bottom.
    pub const fn cell_rect(self) -> (u8, u8, u8, u8) {
        let (x, y) = self.top_left_cell();
        let side = self.side_in_cells();
        (x as u8, y as u8, (x + side - 1) as u8, (y + side - 1) as u8)
    }

    /// Its four children, in reading order: top left, top right, bottom
    /// left, bottom right.
    pub fn children(self) -> [Tile; 4] {
        [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dy)| Tile {
            level: self.level + 1,
            x: self.x * 2 + dx,
            y: self.y * 2 + dy,
        })
    }

    /// The tile one level coarser that holds this one.
    pub const fn parent(self) -> Tile {
        Tile { level: self.level - 1, x: self.x / 2, y: self.y / 2 }
    }

    /// The same-size neighbour in a direction, if it is on the bitmap.
    pub fn neighbour(self, direction: usize) -> Option<Tile> {
        self.neighbour_at(direction, 1)
    }

    /// The same-size tile `distance` tiles away in a direction, if it is
    /// on the bitmap. At distance 2 it is the tile a far copy reads: the
    /// same child position within the neighbour of this tile's parent.
    pub fn neighbour_at(self, direction: usize, distance: usize) -> Option<Tile> {
        let (dx, dy) = DIRECTIONS[direction];
        let distance = distance as isize;
        let (x, y) = (self.x as isize + dx * distance, self.y as isize + dy * distance);
        let across = tiles_across(self.level) as isize;
        (x >= 0 && y >= 0 && x < across && y < across).then_some(Tile { level: self.level, x: x as usize, y: y as usize })
    }

    /// The tile at `level`, coarser or equal, that holds this one.
    pub const fn ancestor(self, level: usize) -> Tile {
        let shift = self.level - level;
        Tile { level, x: self.x >> shift, y: self.y >> shift }
    }

    /// What `bitmap` holds at this tile's top left cell -- the tile's own
    /// value, when every cell of it agrees.
    pub fn top_left_value(self, bitmap: &Bitmap) -> bool {
        let (x, y) = self.top_left_cell();
        bitmap.get(x as u8, y as u8)
    }

    /// Whether any cell of this tile is set in `bitmap`.
    pub fn any_set_in(self, bitmap: &Bitmap) -> bool {
        let (left, top, right, bottom) = self.cell_rect();
        bitmap.any_set_in_rect(left, top, right, bottom)
    }

    /// Sets every cell of this tile in `bitmap`.
    pub fn set_in(self, bitmap: &mut Bitmap) {
        let (left, top, right, bottom) = self.cell_rect();
        bitmap.set_rect(left as i64, top as i64, right as i64, bottom as i64);
    }

    /// Every single cell, in reading order.
    pub fn all_cells() -> impl Iterator<Item = Tile> {
        let across = tiles_across(CELL_LEVEL);
        (0..across).flat_map(move |y| (0..across).map(move |x| Tile { level: CELL_LEVEL, x, y }))
    }

    /// The tiles that fill this one `size_offset` levels finer, in reading
    /// order.
    pub fn tiles_at_size_offset(self, size_offset: usize) -> Vec<Tile> {
        let across = 1usize << size_offset;
        let level = self.level + size_offset;
        let mut out = Vec::with_capacity(across * across);
        for row in 0..across {
            for col in 0..across {
                out.push(Tile { level, x: self.x * across + col, y: self.y * across + row });
            }
        }
        out
    }
}

/// Whether two same-size tiles hold the same cells, a word of a row at a
/// time: a tile's row is a run of `side` bits starting at a multiple of
/// `side`, so it is either whole words or a run inside one word.
pub fn same_cells(bitmap: &Bitmap, a: Tile, b: Tile) -> bool {
    let side = a.side_in_cells();
    let ((a_x, a_y), (b_x, b_y)) = (a.top_left_cell(), b.top_left_cell());
    (0..side).all(|row| {
        let mine = bitmap.row((a_y + row) as u8);
        let theirs = bitmap.row((b_y + row) as u8);
        if side >= u64::BITS as usize {
            let words = side / u64::BITS as usize;
            let (a_word, b_word) = (a_x / u64::BITS as usize, b_x / u64::BITS as usize);
            return (0..words).all(|word| mine[a_word + word] == theirs[b_word + word]);
        }
        let mask = (1u64 << side) - 1;
        let word = |x: usize, row: &[u64]| (row[x / u64::BITS as usize] >> (x % u64::BITS as usize)) & mask;
        word(a_x, mine) == word(b_x, theirs)
    })
}
