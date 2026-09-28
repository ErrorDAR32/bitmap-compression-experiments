//! What a tile is: a size and a place in that size's plane.
//!
//! A tile's level is its size -- level 0 is the whole 256x256 bitmap,
//! level [`CELL_LEVEL`] is one cell -- and its `x` and `y` count tiles
//! of that size, not cells. All three fit a `u8`: levels run 0-8, and
//! no level has more than 256 tiles across. Cell coordinates, also
//! `u8`, are worked out only where cells are actually read.

use crate::{Bitmap, WIDTH};

/// The level of a single cell, the finest there is.
pub const CELL_LEVEL: u8 = 8;

/// Where a tile may copy from: the four same-size neighbours reading
/// order puts before it -- top left, above, top right, left.
pub const DIRECTIONS: [(isize, isize); 4] = [(-1, -1), (0, -1), (1, -1), (-1, 0)];

/// Every direction, as the index into [`DIRECTIONS`] a copy names.
pub fn directions() -> impl Iterator<Item = u8> {
    0..DIRECTIONS.len() as u8
}

/// Tiles across one row of a level's plane.
pub const fn tiles_across(level: u8) -> usize {
    1 << level
}

/// A tile's side at a level, in cells.
pub const fn tile_side(level: u8) -> usize {
    WIDTH >> level
}

/// How many cells one tile at `level` covers.
pub const fn cells_in_tile(level: u8) -> u64 {
    let side = tile_side(level) as u64;
    side * side
}

/// How many levels lie between a tile at `level` and its cells.
pub const fn levels_to_cells(level: u8) -> u8 {
    CELL_LEVEL - level
}

/// A square of the bitmap: its size, and its place among tiles of that
/// size.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tile {
    pub level: u8,
    pub x: u8,
    pub y: u8,
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
    pub const fn top_left_cell(self) -> (u8, u8) {
        let side = self.side_in_cells();
        ((self.x as usize * side) as u8, (self.y as usize * side) as u8)
    }

    /// Its cells as an inclusive rectangle: left, top, right, bottom.
    pub const fn cell_rect(self) -> (u8, u8, u8, u8) {
        let (left, top) = self.top_left_cell();
        let last = (self.side_in_cells() - 1) as u8;
        (left, top, left + last, top + last)
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
    pub fn neighbour(self, direction: u8) -> Option<Tile> {
        self.neighbour_at(direction, 1)
    }

    /// The same-size tile `distance` tiles away in a direction, if it is
    /// on the bitmap. At distance 2 it is the tile a far copy reads: the
    /// same child position within the neighbour of this tile's parent.
    pub fn neighbour_at(self, direction: u8, distance: usize) -> Option<Tile> {
        let (dx, dy) = DIRECTIONS[direction as usize];
        let distance = distance as isize;
        let (x, y) = (self.x as isize + dx * distance, self.y as isize + dy * distance);
        let across = tiles_across(self.level) as isize;
        (x >= 0 && y >= 0 && x < across && y < across).then_some(Tile { level: self.level, x: x as u8, y: y as u8 })
    }

    /// The tile at `level`, coarser or equal, that holds this one.
    pub const fn ancestor(self, level: u8) -> Tile {
        // Up to 8 levels apart, a shift as wide as a u8 itself: done wider.
        let shift = self.level - level;
        Tile { level, x: ((self.x as u16) >> shift) as u8, y: ((self.y as u16) >> shift) as u8 }
    }

    /// What `bitmap` holds at this tile's top left cell -- the tile's own
    /// value, when every cell of it agrees.
    pub fn top_left_value(self, bitmap: &Bitmap) -> bool {
        let (x, y) = self.top_left_cell();
        bitmap.get(x, y)
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

    /// Every tile of one level, in reading order.
    pub fn all_of_level(level: u8) -> impl Iterator<Item = Tile> {
        let last = (tiles_across(level) - 1) as u8;
        (0..=last).flat_map(move |y| (0..=last).map(move |x| Tile { level, x, y }))
    }

    /// Every single cell, in reading order.
    pub fn all_cells() -> impl Iterator<Item = Tile> {
        Tile::all_of_level(CELL_LEVEL)
    }

    /// The tiles that fill this one `size_offset` levels finer, in reading
    /// order.
    pub fn tiles_at_size_offset(self, size_offset: u8) -> Vec<Tile> {
        let across = 1usize << size_offset;
        let level = self.level + size_offset;
        let (first_x, first_y) = (self.x as usize * across, self.y as usize * across);
        let mut out = Vec::with_capacity(across * across);
        for row in 0..across {
            for col in 0..across {
                out.push(Tile { level, x: (first_x + col) as u8, y: (first_y + row) as u8 });
            }
        }
        out
    }
}

/// Whether two same-size tiles hold the same cells, a word of a row at a
/// time.
pub fn same_cells(bitmap: &Bitmap, a: Tile, b: Tile) -> bool {
    const WORD: usize = u64::BITS as usize;
    let side = a.side_in_cells();
    let ((a_x, a_y), (b_x, b_y)) = (a.top_left_cell(), b.top_left_cell());
    let last_row = (side - 1) as u8;
    // A row of a tile is `side` bits from a column that is a multiple of
    // `side`, so it is either whole words or a run inside one word.
    let run = |row: &[u64], x: u8| {
        let x = x as usize;
        (row[x / WORD] >> (x % WORD)) & ((1u64 << side) - 1)
    };
    (0..=last_row).all(|row| {
        let (mine, theirs) = (bitmap.row(a_y + row), bitmap.row(b_y + row));
        if side >= WORD {
            let (a_word, b_word) = (a_x as usize / WORD, b_x as usize / WORD);
            return (0..side / WORD).all(|word| mine[a_word + word] == theirs[b_word + word]);
        }
        run(mine, a_x) == run(theirs, b_x)
    })
}
