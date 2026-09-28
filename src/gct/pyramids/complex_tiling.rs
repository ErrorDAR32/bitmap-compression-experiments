//! The complex tiling: the complex tiler's output, and everything the
//! tree is read from. One 16-bit element per tile, down to single cells:
//!
//! - bits 0-3: what the greedy tiler placed exactly at the tile, in the
//!   placements pyramid's own code;
//! - bits 4-7: the one tile size every cell under the tile is bound at,
//!   plus one -- `0` when the tile is not entirely bound at one size;
//! - bits 8-11: the size offset of the complex tile at the tile -- `0`
//!   when it is not one.
//!
//! The bound size propagates: a placed `Bound` tile is bound at its own
//! size, and any other tile is bound at one size exactly when all four
//! of its children are bound at that same size.

use super::placements::{placement_code, placement_from_code, Placement, Placements, PLACEMENT_CODE_BITS};
use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{Tile, CELL_LEVEL};

const NONE: u64 = 0;
const FIELD_MASK: u64 = 0b1111;
const PLACEMENT_SHIFT: u64 = 0;
const BOUND_SIZE_SHIFT: u64 = PLACEMENT_SHIFT + PLACEMENT_CODE_BITS;
const SIZE_OFFSET_SHIFT: u64 = BOUND_SIZE_SHIFT + 4;

const SHAPE: PyramidShape = PyramidShape { arity: 4, coarsest_level: 0, finest_level: CELL_LEVEL, element_bits: 16 };

fn field(element: u64, shift: u64) -> u64 {
    (element >> shift) & FIELD_MASK
}

fn with_field(element: u64, shift: u64, value: u64) -> u64 {
    (element & !(FIELD_MASK << shift)) | (value << shift)
}

pub trait ComplexTiling {
    /// The greedy tiler's placements, with no complex tiles yet.
    fn complex_tiling(placements: &Pyramid) -> Self;

    /// The tile placed exactly at `tile`, if any.
    fn placed_at(&self, tile: Tile) -> Option<Placement>;

    /// The one tile size every cell under `tile` is bound at, if any.
    fn bound_size(&self, tile: Tile) -> Option<u8>;

    /// Whether every cell under `tile` is bound by tiles of exactly
    /// `size` -- what being unmasked in a complex tile of that
    /// resolution needs.
    fn entirely_bound_at(&self, tile: Tile, size: u8) -> bool {
        self.bound_size(tile) == Some(size)
    }

    /// The size offset of the complex tile at exactly `tile`, if it is one.
    fn complex_tile_size_offset(&self, tile: Tile) -> Option<u8>;

    /// Makes `tile` a complex tile of `size_offset`.
    fn make_complex_tile(&mut self, tile: Tile, size_offset: u8);
}

impl ComplexTiling for Pyramid {
    fn complex_tiling(placements: &Pyramid) -> Self {
        let mut complex_tiling = Pyramid::with_propagation(SHAPE, bound_size_of_children);
        for (tile, placement) in placements.placed_tiles() {
            let bound_size = match placement {
                Placement::Bound(_) => tile.level as u64 + 1,
                Placement::Copied { .. } => NONE,
            };
            let element = with_field(placement_code(placement) << PLACEMENT_SHIFT, BOUND_SIZE_SHIFT, bound_size);
            complex_tiling.set(tile, element);
        }
        complex_tiling
    }

    fn placed_at(&self, tile: Tile) -> Option<Placement> {
        placement_from_code(field(self.get(tile), PLACEMENT_SHIFT))
    }

    fn bound_size(&self, tile: Tile) -> Option<u8> {
        let bound_size = field(self.get(tile), BOUND_SIZE_SHIFT);
        (bound_size != NONE).then(|| (bound_size - 1) as u8)
    }

    fn complex_tile_size_offset(&self, tile: Tile) -> Option<u8> {
        let size_offset = field(self.get(tile), SIZE_OFFSET_SHIFT);
        (size_offset != NONE).then_some(size_offset as u8)
    }

    fn make_complex_tile(&mut self, tile: Tile, size_offset: u8) {
        assert!(size_offset >= 1, "a complex tile's resolution is finer than itself");
        let element = with_field(self.get(tile), SIZE_OFFSET_SHIFT, size_offset as u64);
        self.set(tile, element);
    }
}

/// The propagation: a tile's bound size is its own when a `Bound` tile
/// was placed at it, else its children's when all four share one, else
/// none. Its other fields stay as they are.
fn bound_size_of_children(pyramid: &Pyramid, tile: Tile) -> u64 {
    let element = pyramid.get(tile);
    if let Some(Placement::Bound(_)) = placement_from_code(field(element, PLACEMENT_SHIFT)) {
        return element;
    }
    let sizes: Vec<u64> = pyramid.children_of(tile).into_iter().map(|child| field(pyramid.get(child), BOUND_SIZE_SHIFT)).collect();
    let shared = if sizes.iter().all(|&size| size == sizes[0]) { sizes[0] } else { NONE };
    with_field(element, BOUND_SIZE_SHIFT, shared)
}
