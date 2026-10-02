//! A tile: a size and a place in that size's plane. Its level is its
//! size -- 0 the whole 256x256 bitmap, [`CELL_LEVEL`] one cell -- and its
//! `x` and `y` count tiles of that size, not cells.

use bitmap::morton::{morton_coordinates, morton_index};
use bitmap::{Bitmap, WIDTH};

/// The level of a single cell, the finest there is.
pub const CELL_LEVEL: u8 = 8;
/// The 4x4 floor: the finest tile the tree holds a node at, and the
/// finest a copy reads.
pub const FLOOR_LEVEL: u8 = CELL_LEVEL - 2;
/// A tile's children.
pub const CHILDREN: u8 = 4;
/// Cells in the bitmap.
pub const CELLS: usize = tiles_in_level(CELL_LEVEL);

/// Where a near copy reads from, by direction, in tiles of its own size:
/// the neighbours reading order puts before it -- top left, above, top
/// right, left.
const NEAR_OFFSETS: [(isize, isize); 4] = [(-1, -1), (0, -1), (1, -1), (-1, 0)];
/// Where a far copy reads from, by direction: found by a search over
/// offsets, where the near offsets doubled lost several percent on
/// cities and more on checkerboards.
const FAR_OFFSETS: [(isize, isize); 4] = [(-2, -2), (0, -4), (4, -4), (-4, 0)];
/// The directions a copy names.
pub const DIRECTIONS: u8 = NEAR_OFFSETS.len() as u8;

/// Where a near or far copy in `direction` reads from, in tiles of its
/// own size: always before it in reading order, so decoding has every
/// source before what copies it.
pub const fn copy_offset(far: bool, direction: u8) -> (isize, isize) {
    if far { FAR_OFFSETS[direction as usize] } else { NEAR_OFFSETS[direction as usize] }
}

/// Tiles across one row of a level's plane.
pub const fn tiles_across(level: u8) -> usize {
    1 << level
}

/// Tiles in a level's plane -- and so, too, the tiles filling one tile
/// that many levels finer.
pub const fn tiles_in_level(level: u8) -> usize {
    tiles_across(level) * tiles_across(level)
}

/// How many tiles there are from the whole bitmap down to `level`, both
/// included: `1 + 4 + ... + 4^level`.
pub const fn tiles_down_to(level: u8) -> usize {
    (tiles_in_level(level + 1) - 1) / 3
}

/// How many cells one tile at `level` covers.
pub const fn cells_in_tile(level: u8) -> usize {
    tiles_in_level(CELL_LEVEL - level)
}

/// A square of the bitmap: its size, and its place among tiles of that
/// size.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tile {
    /// Its size: each level half the side of the one before.
    pub level: u8,
    /// Its column among the tiles of its level.
    pub x: u8,
    /// Its row among the tiles of its level.
    pub y: u8,
}

impl Tile {
    /// The whole bitmap as one tile.
    pub const WHOLE_BITMAP: Tile = Tile { level: 0, x: 0, y: 0 };

    /// Its place among its level's tiles, in Morton order.
    pub const fn index(self) -> usize {
        morton_index(self.x, self.y)
    }

    /// Its side, in cells.
    pub const fn side_in_cells(self) -> usize {
        WIDTH >> self.level
    }

    /// Its top left cell.
    pub const fn top_left_cell(self) -> (u8, u8) {
        let side = self.side_in_cells();
        ((self.x as usize * side) as u8, (self.y as usize * side) as u8)
    }

    /// The Morton index of its first cell: its cells are the run from
    /// there.
    pub const fn first_cell(self) -> usize {
        self.index() * cells_in_tile(self.level)
    }

    /// Its four children, in reading order -- which, for one 2x2 group,
    /// is Morton order.
    pub fn children(self) -> [Tile; 4] {
        [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dy)| Tile { level: self.level + 1, x: self.x * 2 + dx, y: self.y * 2 + dy })
    }

    /// The same-size tile `(dx, dy)` tiles away, if it is on the bitmap.
    pub fn offset_by(self, (dx, dy): (isize, isize)) -> Option<Tile> {
        let (x, y) = (self.x as isize + dx, self.y as isize + dy);
        let across = tiles_across(self.level) as isize;
        (x >= 0 && y >= 0 && x < across && y < across).then_some(Tile { level: self.level, x: x as u8, y: y as u8 })
    }

    /// What `bitmap` holds at its top left cell: the tile's value, when
    /// every cell of it agrees.
    pub fn top_left_value(self, bitmap: &Bitmap) -> bool {
        let (x, y) = self.top_left_cell();
        bitmap.get(x, y)
    }

    /// Every tile of one level, in Morton order.
    pub fn all_of_level(level: u8) -> impl Iterator<Item = Tile> {
        Tile::WHOLE_BITMAP.tiles_under(level)
    }

    /// The tiles filling this one `size_offset` levels finer, in Morton
    /// order.
    pub fn tiles_under(self, size_offset: u8) -> impl Iterator<Item = Tile> {
        let count = tiles_in_level(size_offset);
        let level = self.level + size_offset;
        (self.index() * count..(self.index() + 1) * count).map(move |index| {
            let (x, y) = morton_coordinates(index);
            Tile { level, x, y }
        })
    }
}

/// One element per tile, at every level from the whole bitmap down to
/// `FINEST`, each level's elements in Morton order: a tile's four
/// children are four consecutive elements.
#[derive(Clone)]
pub struct Pyramid<T, const FINEST: u8> {
    /// Every level's elements, coarsest first.
    elements: Box<[T]>,
}

impl<T: Copy + Default, const FINEST: u8> Pyramid<T, FINEST> {
    /// Every element the default.
    pub fn new() -> Self {
        Self { elements: vec![T::default(); tiles_down_to(FINEST)].into_boxed_slice() }
    }

    /// Where `level`'s elements start.
    const fn level_start(level: u8) -> usize {
        (tiles_in_level(level) - 1) / 3
    }

    /// The element of `level`'s tile at Morton index `index`.
    #[inline]
    fn slot(level: u8, index: usize) -> usize {
        Self::level_start(level) + index
    }

    /// `tile`'s element.
    #[inline]
    pub fn get(&self, tile: Tile) -> T {
        self.elements[Self::slot(tile.level, tile.index())]
    }

    /// Makes `value` `tile`'s element.
    #[inline]
    pub fn set(&mut self, tile: Tile, value: T) {
        self.set_at(tile.level, tile.index(), value);
    }

    /// Makes `value` the element of `level`'s tile at Morton index
    /// `index`.
    #[inline]
    pub fn set_at(&mut self, level: u8, index: usize, value: T) {
        self.elements[Self::slot(level, index)] = value;
    }

    /// `tile`'s four children's elements, in reading order.
    #[inline]
    pub fn children(&self, tile: Tile) -> [T; 4] {
        self.children_at(tile.level, tile.index())
    }

    /// The four children's elements of `level`'s tile at Morton index
    /// `index`, in reading order.
    #[inline]
    pub fn children_at(&self, level: u8, index: usize) -> [T; 4] {
        let first = Self::slot(level + 1, 4 * index);
        self.elements[first..first + 4].try_into().expect("four children")
    }
}

impl<T: Copy + Default, const FINEST: u8> Default for Pyramid<T, FINEST> {
    /// The same as [`Pyramid::new`].
    fn default() -> Self {
        Self::new()
    }
}
