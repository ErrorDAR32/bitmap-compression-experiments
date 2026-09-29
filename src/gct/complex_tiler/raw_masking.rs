//! Which parts a complex tile of 1x1 resolution masks: every part
//! cheaper said by itself -- as the nodes the greedy tiler's tiles make
//! of it -- than as its raw cells.
//!
//! Decided once a bitmap, top-down, from the greedy tiler's placements
//! before the complex tiling is built: a tile said by itself costs its
//! own node, whose parts are in turn each said the cheaper way; a tile
//! said raw costs its cells. Inside a raw complex tile every part pays
//! one mask bit either way, and nothing else is counted -- no other
//! complex tile, no binding above. The complex tiler then weighs a raw
//! complex tile by its exact bit cost like any other; this only settles
//! what it would mask.

use super::bit_cost::payload_bits;
use crate::gct::grammar::*;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::pyramids::placements::Placement;
use crate::gct::tile::{cells_in_tile, Tile, CELL_LEVEL, FLOOR_LEVEL};

/// Marks, in the complex tiling, the tiles a complex tile of 1x1
/// resolution masks, read off what the greedy tiler placed in it.
/// Decided top-down, from the whole bitmap, only where a tile's cost
/// depends on what is under it: under a tile placed whole nothing is
/// ever asked.
pub(crate) fn decide_raw_masking(complex_tiling: &mut ComplexTiling) {
    cost_in_raw(complex_tiling, Tile::whole_bitmap());
}

/// What `tile` costs in a raw complex tile, its mask bit included: the
/// cheaper of its raw cells and itself, its parts each costed the same
/// way. Marks `tile` raw masked when itself is cheaper.
fn cost_in_raw(complex_tiling: &mut ComplexTiling, tile: Tile) -> u64 {
    let raw = cells_in_tile(tile.level);
    if tile.level == CELL_LEVEL {
        // A cell is only ever raw.
        return MASK_BIT_WIDTH as u64 + raw;
    }
    let placement = complex_tiling.placed_at(tile);
    let by_itself = said_by_itself(placement, tile, |part| cost_in_raw(complex_tiling, part));
    if by_itself < raw {
        complex_tiling.mark_raw_masked(tile);
    }
    MASK_BIT_WIDTH as u64 + by_itself.min(raw)
}

/// What `tile`, coarser than a cell, with `placement` placed at it,
/// costs said by itself, its parts each at `part_cost`.
fn said_by_itself(placement: Option<Placement>, tile: Tile, mut part_cost: impl FnMut(Tile) -> u64) -> u64 {
    let leaf_bind = (LEAF_WIDTH + CODE_WIDTH) as u64;
    match placement {
        // In the raw complex tile's body: no size offset.
        Some(Placement::Bound { masked_children: 0, .. }) => leaf_bind + bind_resolution_width(tile.level, true) as u64 + payload_bits(0),
        Some(bind @ Placement::Bound { .. }) => {
            MASKING_DIVIDE_HEADER_WIDTH as u64 + masked_parts(tile, bind, &mut part_cost)
        }
        Some(copy @ Placement::Copied { .. }) => {
            let mut bits = leaf_bind + (FAR_WIDTH + DIRECTION_WIDTH) as u64;
            if copy_or_divide_may_mask(tile.level) {
                bits += MASK_PRESENT_WIDTH as u64;
            }
            if copy.masks_any() {
                bits += masked_parts(tile, copy, &mut part_cost);
            }
            bits
        }
        // At the 4x4 floor, a residual block.
        None if tile.level == FLOOR_LEVEL => (LEAF_WIDTH + RESIDUAL_BLOCK_BITS) as u64,
        None => {
            let mut bits = LEAF_WIDTH as u64;
            if copy_or_divide_may_mask(tile.level) {
                bits += MASK_PRESENT_WIDTH as u64;
            }
            bits + tile.children().into_iter().map(part_cost).sum::<u64>()
        }
    }
}

/// A masking placement's child mask, and the children it masks, each at
/// `part_cost`.
fn masked_parts(tile: Tile, placement: Placement, part_cost: &mut impl FnMut(Tile) -> u64) -> u64 {
    let masked = tile.children().into_iter().filter(|&child| placement.masks(child));
    CHILD_MASK_WIDTH as u64 + masked.map(part_cost).sum::<u64>()
}
