//! Which parts a complex tile of 1x1 resolution masks: every part
//! cheaper said by itself -- as the nodes the greedy tiler's tiles make
//! of it -- than as its raw cells.
//!
//! Decided once a bitmap, top-down, from the greedy tiler's placements
//! before the complex tiling is built: a tile
//! said by itself costs its own node, whose parts are in turn each
//! said the cheaper way; a tile said raw costs its cells. Inside a raw
//! complex tile every part pays one mask bit either way, and nothing
//! else is counted -- no other complex tile, no binding above. The
//! complex tiler then weighs a raw complex tile by its exact bit cost
//! like any other; this only settles what it would mask.

use crate::gct::grammar::*;
use crate::gct::pyramids::placements::{Placement, Placements};
use crate::gct::pyramids::pyramid::Pyramid;
use crate::fixed_list::FixedList;
use crate::gct::tile::{cells_in_tile, tiles_down_to, Tile, CELL_LEVEL};

/// The most tiles a complex tile of 1x1 resolution masks: every tile
/// from the whole bitmap down to the 2x2 floor -- nothing finer is
/// placed.
pub const MOST_RAW_MASKED: usize = tiles_down_to(CELL_LEVEL - 1);

/// The tiles a complex tile of 1x1 resolution masks, read off what the
/// greedy tiler placed, into `masked`, whatever it held before. Decided top-down, from the whole bitmap, only
/// where a tile's cost depends on what is under it: under a tile placed
/// whole nothing is ever asked.
pub(crate) fn decide_raw_masking(placements: &Pyramid, masked: &mut FixedList<Tile, MOST_RAW_MASKED>) {
    masked.clear();
    cost_in_raw(placements, Tile::whole_bitmap(), masked);
}

/// What `tile` costs in a raw complex tile, its mask bit included: the
/// cheaper of its raw cells and itself, its parts each costed the same
/// way. Adds `tile` to `masked` when itself is cheaper.
fn cost_in_raw(placements: &Pyramid, tile: Tile, masked: &mut FixedList<Tile, MOST_RAW_MASKED>) -> u64 {
    let raw = cells_in_tile(tile.level);
    if tile.level == CELL_LEVEL {
        // A cell is only ever raw.
        return MASK_BIT_WIDTH as u64 + raw;
    }
    let by_itself = said_by_itself(placements, tile, |part| cost_in_raw(placements, part, masked));
    if by_itself < raw {
        masked.push(tile);
    }
    MASK_BIT_WIDTH as u64 + by_itself.min(raw)
}

/// What `tile`, coarser than a cell, costs said by itself, its parts
/// each at `part_cost`.
fn said_by_itself(placements: &Pyramid, tile: Tile, mut part_cost: impl FnMut(Tile) -> u64) -> u64 {
    let leaf_bind = (LEAF_WIDTH + CODE_WIDTH) as u64;
    if tile.level == CELL_LEVEL - 1 {
        // The 2x2 floor: a tile and its value, or a residual.
        let value_or_cells = match placements.placement(tile) {
            Some(placement) if placement.is_whole_bind() => 1,
            _ => cells_in_tile(tile.level),
        };
        return LEAF_WIDTH as u64 + value_or_cells;
    }
    match placements.placement(tile) {
        Some(Placement::Bound { masked_children: 0, .. }) => leaf_bind + resolution_width(tile.level) as u64 + 1,
        Some(bind @ Placement::Bound { .. }) => {
            (LEAF_WIDTH + MASK_PRESENT_WIDTH + FLIP_WIDTH) as u64 + masked_parts(tile, bind, &mut part_cost)
        }
        Some(copy @ Placement::Copied { .. }) => {
            let mut bits = leaf_bind + (FAR_WIDTH + DIRECTION_WIDTH) as u64;
            if copy_may_mask(tile.level) {
                bits += MASK_PRESENT_WIDTH as u64;
            }
            if copy.masks_any() {
                bits += masked_parts(tile, copy, &mut part_cost);
            }
            bits
        }
        None => {
            let mut bits = LEAF_WIDTH as u64;
            if divide_may_mask(tile.level) {
                bits += MASK_PRESENT_WIDTH as u64;
            }
            bits + tile.children().into_iter().map(part_cost).sum::<u64>()
        }
    }
}

/// A masking placement's child mask, and the children it masks, each at
/// `part_cost`.
fn masked_parts(tile: Tile, placement: Placement, part_cost: &mut impl FnMut(Tile) -> u64) -> u64 {
    tile.children()
        .into_iter()
        .map(|child| MASK_BIT_WIDTH as u64 + if placement.masks(child) { part_cost(child) } else { 0 })
        .sum()
}
