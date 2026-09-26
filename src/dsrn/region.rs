//! A square of the quadtree.
//!
//! Everything the encoding describes is one of these: the whole
//! bitmap is the root, its four quadrants are its children, and a
//! single cell is a leaf. A region is always aligned to its own size,
//! which is why a tile is always some region and a tile that needs
//! describing on its own can describe itself.

/// A square of the quadtree: side `1 << level`, at `(x, y)` in units
/// of that side.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Region {
    pub(crate) level: usize,
    pub(crate) x: usize,
    pub(crate) y: usize,
}

/// Where a region may copy from: the four neighbours of its own size
/// that reading order has already passed.
pub(crate) const DIRECTIONS: [(isize, isize); 4] = [(-1, -1), (0, -1), (1, -1), (-1, 0)];

/// The four children of a region, in reading order: top left, top
/// right, bottom left, bottom right.
pub(crate) const CHILDREN: [(usize, usize); 4] = [(0, 0), (1, 0), (0, 1), (1, 1)];

pub(crate) fn children_of(region: Region) -> [Region; 4] {
    CHILDREN.map(|(dx, dy)| Region {
        level: region.level - 1,
        x: region.x * 2 + dx,
        y: region.y * 2 + dy,
    })
}
