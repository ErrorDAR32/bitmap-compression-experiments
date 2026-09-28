//! The complex tiling: the complex tiler's output, and everything the
//! tree is read from. One 32-bit element per tile, down to single cells:
//!
//! - bits 0-7: what the greedy tiler placed exactly at the tile, in the
//!   placement code ([`super::placements`]) -- the greedy tiler's
//!   output, written before any other bit;
//! - bits 8-11: the one tile size every cell under the tile is bound at,
//!   plus one -- `0` when the tile is not entirely bound at one size;
//! - bits 12-15: the size offset of the complex tile at the tile -- `0`
//!   when it is not one;
//! - bit 16: whether a complex tile of 1x1 resolution masks the tile --
//!   it is cheaper said by itself than raw. The complex tiler decides
//!   it, once a bitmap ([`crate::gct::complex_tiler::raw_masking`]);
//! - bits 17-25: the sizes of the whole binds placed at or under the
//!   tile, bit `n` for size `n` -- a complex tile has nothing to unmask
//!   at a resolution none is placed at.
//!
//! The bound size propagates: a placed `Bound` tile is bound at its own
//! size, a placed copy or a bind that masks at none, and any other tile
//! is bound at one size exactly when all four of its children are bound
//! at that same size; the sizes of the binds under a tile are its own
//! whole bind's, or all of its children's.

use super::placements::{placement_code, placement_from_code, Placement, Placements, FINEST_MASKING_LEVEL, PLACEMENT_CODE_BITS};
use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::tile::{Tile, CELL_LEVEL};

const NONE: u64 = 0;

/// Where a field sits in an element, and how wide it is.
#[derive(Clone, Copy)]
struct Field {
    shift: u64,
    bits: u64,
}

const PLACEMENT: Field = Field { shift: 0, bits: PLACEMENT_CODE_BITS };
/// Enough for a level plus one, up to `CELL_LEVEL + 1`.
const BOUND_SIZE: Field = Field { shift: PLACEMENT.shift + PLACEMENT.bits, bits: 4 };
/// Enough for a size offset, up to `CELL_LEVEL`.
const SIZE_OFFSET: Field = Field { shift: BOUND_SIZE.shift + BOUND_SIZE.bits, bits: 4 };
const RAW_MASKS: Field = Field { shift: SIZE_OFFSET.shift + SIZE_OFFSET.bits, bits: 1 };
/// One bit a size, `CELL_LEVEL + 1` of them.
const BOUND_SIZES_UNDER: Field = Field { shift: RAW_MASKS.shift + RAW_MASKS.bits, bits: CELL_LEVEL as u64 + 1 };
const YES: u64 = 1;


const SHAPE: PyramidShape = PyramidShape { arity: 4, coarsest_level: 0, finest_level: CELL_LEVEL, element_bits: 32 };

fn field(element: u64, field: Field) -> u64 {
    (element >> field.shift) & ((1 << field.bits) - 1)
}

fn with_field(element: u64, field: Field, value: u64) -> u64 {
    let mask = ((1 << field.bits) - 1) << field.shift;
    (element & !mask) | (value << field.shift)
}

pub trait ComplexTiling {
    /// The greedy tiler's placements, now kept in step, with no complex
    /// tiles yet, and `raw_masked` the tiles a complex tile of 1x1
    /// resolution masks.
    fn complex_tiling(placements: Pyramid, raw_masked: &[Tile]) -> Self;

    /// The tile placed exactly at `tile`, if any.
    fn placed_at(&self, tile: Tile) -> Option<Placement>;

    /// The one tile size every cell under `tile` is bound at, if any.
    fn bound_size(&self, tile: Tile) -> Option<u8>;

    /// Whether every cell under `tile` is bound by tiles of exactly
    /// `size` -- what being unmasked in a complex tile of that
    /// resolution needs. At 1x1, every cell is a tile of its own, so a
    /// complex tile of 1x1 resolution says cells raw -- but not a part
    /// cheaper said by itself: that part it masks.
    fn entirely_bound_at(&self, tile: Tile, size: u8) -> bool {
        if size == CELL_LEVEL {
            return !self.raw_masks(tile);
        }
        self.bound_size(tile) == Some(size)
    }

    /// Whether a complex tile of 1x1 resolution masks `tile`.
    fn raw_masks(&self, tile: Tile) -> bool;

    /// Whether any whole bind of `size` is placed at or under `tile`.
    fn any_bound_under(&self, tile: Tile, size: u8) -> bool;



    /// Whether `tile`, a child of a divide nested in `nested`, is left to
    /// the binding above it, of `bound_above`: the divide masks (8x8 or
    /// coarser), `tile` is bound whole to that value, and unmasked in no
    /// complex tile it is nested in -- which would say it for a bit, where
    /// the binding above says it for none.
    fn left_to_binding_above(&self, tile: Tile, bound_above: bool, nested: &NestedResolutions) -> bool;

    /// The size offset of the complex tile at exactly `tile`, if it is one.
    fn complex_tile_size_offset(&self, tile: Tile) -> Option<u8>;

    /// Makes `tile` a complex tile of `size_offset`.
    fn make_complex_tile(&mut self, tile: Tile, size_offset: u8);

    /// Makes `tile` no complex tile.
    fn clear_complex_tile(&mut self, tile: Tile);
}

impl ComplexTiling for Pyramid {
    fn complex_tiling(placements: Pyramid, raw_masked: &[Tile]) -> Self {
        let raw_masks: Vec<(Tile, u64)> =
            raw_masked.iter().map(|&tile| (tile, with_field(placements.get(tile), RAW_MASKS, YES))).collect();
        let mut complex_tiling = placements.with_propagation_set(bound_size_of_children);
        complex_tiling.set_all(raw_masks);
        complex_tiling
    }

    fn placed_at(&self, tile: Tile) -> Option<Placement> {
        placement_from_code(field(self.get(tile), PLACEMENT))
    }

    fn bound_size(&self, tile: Tile) -> Option<u8> {
        let bound_size = field(self.get(tile), BOUND_SIZE);
        (bound_size != NONE).then(|| (bound_size - 1) as u8)
    }

    fn any_bound_under(&self, tile: Tile, size: u8) -> bool {
        field(self.get(tile), BOUND_SIZES_UNDER) & (1 << size) != 0
    }

    fn raw_masks(&self, tile: Tile) -> bool {
        field(self.get(tile), RAW_MASKS) == YES
    }



    fn left_to_binding_above(&self, tile: Tile, bound_above: bool, nested: &NestedResolutions) -> bool {
        tile.level <= FINEST_MASKING_LEVEL + 1
            && matches!(self.placed_at(tile), Some(Placement::Bound { value, masked_children: 0 }) if value == bound_above)
            && nested.unmasking(self, tile).is_none()
    }

    fn complex_tile_size_offset(&self, tile: Tile) -> Option<u8> {
        let size_offset = field(self.get(tile), SIZE_OFFSET);
        (size_offset != NONE).then_some(size_offset as u8)
    }

    fn make_complex_tile(&mut self, tile: Tile, size_offset: u8) {
        assert!(size_offset >= 1, "a complex tile's resolution is finer than itself");
        let element = with_field(self.get(tile), SIZE_OFFSET, size_offset as u64);
        self.set(tile, element);
    }

    fn clear_complex_tile(&mut self, tile: Tile) {
        let element = with_field(self.get(tile), SIZE_OFFSET, NONE);
        self.set(tile, element);
    }
}

/// The placements are the placement bits of this same pyramid, before
/// it is kept in step: placing a whole bind also records its own bound
/// size and size, which the complex tiling then carries up.
impl Placements for Pyramid {
    fn placements() -> Self {
        Pyramid::new(SHAPE)
    }

    fn placement(&self, tile: Tile) -> Option<Placement> {
        placement_from_code(field(self.get(tile), PLACEMENT))
    }

    fn place(&mut self, tile: Tile, placement: Placement) {
        let mut element = with_field(self.get(tile), PLACEMENT, placement_code(placement));
        if placement.is_whole_bind() {
            element = with_field(element, BOUND_SIZE, tile.level as u64 + 1);
            element = with_field(element, BOUND_SIZES_UNDER, 1 << tile.level);
        }
        self.set(tile, element);
    }

    fn placed_tiles(&self) -> impl Iterator<Item = (Tile, Placement)> + '_ {
        (0..=CELL_LEVEL).flat_map(move |level| {
            self.tiles_of_level(level).filter_map(move |tile| self.placement(tile).map(|placement| (tile, placement)))
        })
    }
}

/// The propagation: a tile's bound size is its own when a whole bind
/// was placed at it, none when anything else was, else its children's
/// when all four share one, else none. Its other fields stay as they
/// are.
fn bound_size_of_children(pyramid: &Pyramid, tile: Tile) -> u64 {
    let element = pyramid.get(tile);
    let placement = placement_from_code(field(element, PLACEMENT));
    if placement.is_some_and(Placement::is_whole_bind) {
        return element;
    }
    // The children's one bound size, if they share one, and every size
    // bound under any of them.
    let (mut shared, mut under) = (None, NONE);
    for child in pyramid.children_of(tile) {
        let child = pyramid.get(child);
        let size = field(child, BOUND_SIZE);
        shared = match shared {
            None => Some(size),
            Some(before) if before == size => Some(size),
            Some(_) => Some(NONE),
        };
        under |= field(child, BOUND_SIZES_UNDER);
    }
    let shared = if placement.is_some() { NONE } else { shared.unwrap_or(NONE) };
    with_field(with_field(element, BOUND_SIZE, shared), BOUND_SIZES_UNDER, under)
}
