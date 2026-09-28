//! Which parts a complex tile of 1x1 resolution masks: every part
//! cheaper said by itself -- as the nodes the greedy tiler's tiles make
//! of it -- than as its raw cells.
//!
//! Decided once a bitmap, bottom-up, before any complex tile: a tile
//! said by itself costs its own node, whose parts are in turn each
//! said the cheaper way; a tile said raw costs its cells. Inside a raw
//! complex tile every part pays one mask bit either way, and nothing
//! else is counted -- no other complex tile, no binding above. The
//! complex tiler then weighs a raw complex tile by its exact bit cost
//! like any other; this only settles what it would mask.

use crate::gct::grammar::*;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::pyramids::placements::Placement;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{cells_in_tile, tiles_across, Tile, CELL_LEVEL};

/// Records, for every tile, whether a complex tile of 1x1 resolution
/// masks it.
pub fn decide_raw_masking(complex_tiling: &mut Pyramid) {
    // Per level, finest first, what each tile costs in a raw complex
    // tile, its mask bit included: the cheaper of raw and itself.
    let mut finer: Vec<u64> = Vec::new();
    for level in (0..=CELL_LEVEL).rev() {
        let mut costs = Vec::with_capacity(tiles_across(level) * tiles_across(level));
        for tile in Tile::all_of_level(level) {
            let child_cost = |child: Tile| finer[child.y as usize * tiles_across(child.level) + child.x as usize];
            let raw = cells_in_tile(level);
            let by_itself = said_by_itself(complex_tiling, tile, child_cost);
            let masks = by_itself.is_some_and(|bits| bits < raw);
            complex_tiling.set_raw_masks(tile, masks);
            costs.push(MASK_BIT_WIDTH as u64 + by_itself.map_or(raw, |bits| bits.min(raw)));
        }
        finer = costs;
    }
}

/// What `tile` costs said by itself, its parts each at `part_cost` --
/// none for a cell, which only the residual pass or a raw complex tile
/// says.
fn said_by_itself(complex_tiling: &Pyramid, tile: Tile, part_cost: impl Fn(Tile) -> u64) -> Option<u64> {
    let leaf_bind = (LEAF_WIDTH + CODE_WIDTH) as u64;
    let masked_parts = |placement: Placement| -> u64 {
        tile.children()
            .into_iter()
            .map(|child| MASK_BIT_WIDTH as u64 + if placement.masks(child) { part_cost(child) } else { 0 })
            .sum()
    };
    if tile.level == CELL_LEVEL {
        return None;
    }
    if tile.level == CELL_LEVEL - 1 {
        // The 2x2 floor: a tile and its value, or a residual.
        let value_or_cells = match complex_tiling.placed_at(tile) {
            Some(placement) if placement.is_whole_bind() => 1,
            _ => cells_in_tile(tile.level),
        };
        return Some(LEAF_WIDTH as u64 + value_or_cells);
    }
    Some(match complex_tiling.placed_at(tile) {
        Some(Placement::Bound { masked_children: 0, .. }) => leaf_bind + resolution_width(tile.level) as u64 + 1,
        Some(bind @ Placement::Bound { .. }) => (LEAF_WIDTH + MASK_PRESENT_WIDTH + FLIP_WIDTH) as u64 + masked_parts(bind),
        Some(copy @ Placement::Copied { .. }) => {
            let mut bits = leaf_bind + (FAR_WIDTH + DIRECTION_WIDTH) as u64;
            if copy_may_mask(tile.level) {
                bits += MASK_PRESENT_WIDTH as u64;
            }
            if copy.masks_any() {
                bits += masked_parts(copy);
            }
            bits
        }
        None => {
            let mut bits = LEAF_WIDTH as u64;
            if divide_may_mask(tile.level) {
                bits += MASK_PRESENT_WIDTH as u64;
            }
            bits + tile.children().into_iter().map(&part_cost).sum::<u64>()
        }
    })
}
