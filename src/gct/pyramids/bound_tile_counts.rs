//! For every tile and every tile size: how many `Bound` tiles of
//! exactly that size the greedy tiler placed under it.
//!
//! One pyramid per size, each running from the whole bitmap down to
//! that size: each placed `Bound` tile of that size sets its own element
//! to 1, and the pyramid propagates each set upward by summing a tile's
//! children. Placed tiles never overlap, so a sum counts each one
//! once. It depends only on what the greedy tiler placed, so it is
//! built once a bitmap.
//!
//! 1x1 tiles are left out entirely: they are always the residual
//! pass's own, never part of a complex tile.

use super::placements::{Placement, Placements};
use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{Tile, CELL_LEVEL};

/// Enough for every 2x2 tile of the whole bitmap, 4^7.
const COUNT_BITS: usize = 16;

const fn shape(size: u8) -> PyramidShape {
    PyramidShape { arity: 4, coarsest_level: 0, finest_level: size, element_bits: COUNT_BITS }
}

/// One pyramid per tile size, indexed by that size: level 0 (the whole
/// bitmap) to `CELL_LEVEL - 1` (2x2).
pub trait BoundTileCounts {
    /// Counts what `placements` holds.
    fn bound_tile_counts(placements: &Pyramid) -> Self;

    /// How many `Bound` tiles of exactly `size` lie under `tile` -- none
    /// when `size` is coarser than `tile` itself.
    fn under(&self, tile: Tile, size: u8) -> u32;

    /// Whether every tile of `size` under `tile` is a `Bound` tile
    /// placed at exactly that size.
    fn entirely_bound_at(&self, tile: Tile, size: u8) -> bool {
        self.under(tile, size) == 1 << (2 * (size - tile.level))
    }
}

impl BoundTileCounts for Vec<Pyramid> {
    fn bound_tile_counts(placements: &Pyramid) -> Self {
        let mut by_size: Vec<Pyramid> =
            (0..CELL_LEVEL).map(|size| Pyramid::with_propagation(shape(size), sum_of_children)).collect();
        for (tile, placement) in placements.placed_tiles() {
            if tile.level < CELL_LEVEL && matches!(placement, Placement::Bound(_)) {
                by_size[tile.level as usize].set(tile, 1);
            }
        }
        by_size
    }

    fn under(&self, tile: Tile, size: u8) -> u32 {
        if size < tile.level {
            return 0;
        }
        self[size as usize].get(tile) as u32
    }
}

/// The propagation: how many `Bound` tiles of this size lie under a tile
/// is the sum over its children.
fn sum_of_children(pyramid: &Pyramid, tile: Tile) -> u64 {
    pyramid.children_of(tile).into_iter().map(|child| pyramid.get(child)).sum()
}
