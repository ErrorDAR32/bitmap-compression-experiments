//! What a tile costs to say, in bits, as the complex tiling stands: the
//! grammar's own widths ([`crate::tessera::grammar`]) applied to what the
//! tiling holds at each tile -- the same rules the tree is read by, so
//! the count is the encoder's own, for any tiling: the reference the
//! greedy tiler's own counts ([`greedy_tiler`](mod@crate::tessera::greedy_tiler)) are held to
//! in debug builds. One thing is counted at a price: a residual block's
//! cells, at what the last pass takes for them ([`ResidualPrices`]). The last pass codes
//! them from the cells around them, so what they take depends on the
//! whole pass, and is known only by coding it.
//!
//! Every check in `tests/common` holds this to the encoder's count
//! (`crate::diagnostics::examination` gathers both), on every bitmap
//! tested.

use crate::tessera::grammar::cell_list;
use crate::tessera::grammar::*;
use crate::tessera::pyramids::complex_tiling::{ComplexTiling, Fields};
use crate::tessera::pyramids::placements::Placement;
use crate::tessera::residual_prices::ResidualPrices;
use crate::tessera::tile::{tiles_in_level, Tile, CHILDREN, FLOOR_LEVEL};
use crate::Bitmap;

/// How many resolution tiles a tile holds `size_offset` levels finer:
/// one payload bit each.
pub fn payload_bits(size_offset: u8) -> u64 {
    tiles_in_level(size_offset) as u64
}

/// What a count reads: the tiling counted, the bitmap its payloads and
/// cell lists say, and what the last pass takes for each residual block.
#[derive(Clone, Copy)]
pub struct Counting<'a> {
    /// The tiling counted.
    pub complex_tiling: &'a ComplexTiling,
    /// The bitmap.
    pub bitmap: &'a Bitmap,
    /// Each residual block's bits in the last pass.
    pub residual_prices: &'a ResidualPrices,
}

/// The bits `tile` costs, `bound_above` the value bound above it, and
/// everything under it, as the tiling stands: the reference count, each
/// tile counted once, which the tests hold to the encoder's.
pub fn bits(counting: Counting, tile: Tile, bound_above: bool) -> u64 {
    bits_with(counting, tile, counting.complex_tiling.fields(tile), bound_above)
}

/// The bits a complex tile at a tile of `level`, of `size_offset`,
/// spends before its payload: its leaf and code bits, its size offset
/// and, at 1x1 resolution, its payload mode.
pub fn complex_tile_header_bits(level: u8, size_offset: u8) -> u64 {
    let payload_mode = if has_payload_mode(level, size_offset) { PAYLOAD_MODE_WIDTH } else { 0 };
    (LEAF_WIDTH + CODE_WIDTH + size_offset_bits(level, size_offset) + payload_mode) as u64
}

/// The bits the whole tree takes, as the tiling stands, starting at
/// `start_level`: the start level, then every tile of that level --
/// residual blocks at their prices.
pub fn tree_bits(counting: Counting, start_level: u8) -> u64 {
    START_LEVEL_WIDTH as u64
        + Tile::all_of_level(start_level)
            .map(|tile| bits(counting, tile, BOUND_AT_THE_TOP))
            .sum::<u64>()
}

/// [`bits`], with `here` as `tile`'s fields.
pub fn bits_with(counting: Counting, tile: Tile, here: Fields, bound_above: bool) -> u64 {
    node_bits(counting, tile, here, bound_above, &mut |child, fields, bound_above| bits_with(counting, child, fields, bound_above))
}

/// The bits `tile` costs with `here` as its fields -- which need not be
/// the tiling's: a candidate is scored as the complex tile it would be,
/// with nothing changed -- `bound_above` the value bound above it: its
/// own bits, and each child node's as `child_bits(child, its fields,
/// bound_above)` gives them -- the four children's fields read
/// together, once.
pub fn node_bits(
    counting: Counting,
    tile: Tile,
    here: Fields,
    bound_above: bool,
    child_bits: &mut impl FnMut(Tile, Fields, bool) -> u64,
) -> u64 {
    let complex_tiling = counting.complex_tiling;
    let leaf_bind = (LEAF_WIDTH + CODE_WIDTH) as u64;
    match here.placed() {
        Some(Placement::Bound { masked_children: 0, .. }) => complex_tile_header_bits(tile.level, 0) + payload_bits(0),
        Some(bind @ Placement::Bound { .. }) => {
            // Spelled as a divide that masks and flips the value bound above.
            MASKING_DIVIDE_HEADER_WIDTH as u64 + masked_children_bits(complex_tiling, tile, bind, bound_above, child_bits)
        }
        Some(copy @ Placement::Copied { .. }) => {
            let mut copy_bits = leaf_bind + (FAR_WIDTH + DIRECTION_WIDTH) as u64;
            if copy_or_divide_may_mask(tile.level) {
                copy_bits += MASK_PRESENT_WIDTH as u64;
            }
            if copy.masks_any() {
                copy_bits += masked_children_bits(complex_tiling, tile, copy, bound_above, child_bits);
            }
            copy_bits
        }
        None => match here.complex_tile_size_offset() {
            Some(size_offset) if here.is_cell_list() => complex_tile_header_bits(tile.level, size_offset) + cell_list::bits(counting.bitmap, tile, cell_list::set_count(counting.bitmap, tile)),
            Some(size_offset) => {
                debug_assert!(here.entirely_bound_at(tile.level + size_offset), "{tile:?}: a complex tile not entirely bound at its resolution");
                complex_tile_header_bits(tile.level, size_offset) + payload_bits(size_offset)
            }
            None if tile.level == FLOOR_LEVEL => {
                // A residual block, and its cells in the last pass.
                LEAF_WIDTH as u64 + counting.residual_prices.of(tile)
            }
            None => {
                let (children, fields) = (tile.children(), complex_tiling.children_fields(tile));
                let left: [bool; CHILDREN as usize] =
                    std::array::from_fn(|child_index| fields[child_index].left_to_binding_above(children[child_index], bound_above));
                // A divide that leaves any child to the binding above masks,
                // keeping the value bound above.
                let mut divide_bits = divide_bits(tile.level, left.contains(&true));
                for child_index in 0..children.len() {
                    if !left[child_index] {
                        divide_bits += child_bits(children[child_index], fields[child_index], bound_above);
                    }
                }
                divide_bits
            }
        },
    }
}

/// A masking placement's child mask, and each child it masks as
/// `child_bits` gives it, `bound_above` the value bound above the
/// placement.
fn masked_children_bits(
    complex_tiling: &ComplexTiling,
    tile: Tile,
    placed: Placement,
    bound_above: bool,
    child_bits: &mut impl FnMut(Tile, Fields, bool) -> u64,
) -> u64 {
    let bound_inside = placed.bound_inside(bound_above);
    let mut bits = CHILD_MASK_WIDTH as u64;
    for (child, fields) in tile.children().into_iter().zip(complex_tiling.children_fields(tile)) {
        if placed.masks(child) {
            bits += child_bits(child, fields, bound_inside);
        }
    }
    bits
}

/// What a divide that leaves children to the binding above spends on
/// that and a divide that leaves none does not: its flip bit and child
/// mask.
const LEAVING_BITS: u64 = (FLIP_WIDTH + CHILD_MASK_WIDTH) as u64;

/// The bits `tile`, whose fields are `here`, takes itself as the tiling
/// stands, its counted children's bits aside; `left` the children a
/// divide leaves to the binding above; a residual block at its price in
/// `residual_prices`.
pub fn own_bits(tile: Tile, here: Fields, left: u8, residual_prices: &ResidualPrices) -> u64 {
    let leaf_and_code = (LEAF_WIDTH + CODE_WIDTH) as u64;
    let mask_present = if copy_or_divide_may_mask(tile.level) { MASK_PRESENT_WIDTH as u64 } else { 0 };
    match here.placed() {
        Some(Placement::Bound { masked_children: 0, .. }) => complex_tile_header_bits(tile.level, 0) + payload_bits(0),
        // Spelled as a divide that masks and flips the value bound above.
        Some(Placement::Bound { .. }) => (MASKING_DIVIDE_HEADER_WIDTH + CHILD_MASK_WIDTH) as u64,
        Some(copy @ Placement::Copied { .. }) => {
            let child_mask = if copy.masks_any() { CHILD_MASK_WIDTH as u64 } else { 0 };
            leaf_and_code + (FAR_WIDTH + DIRECTION_WIDTH) as u64 + mask_present + child_mask
        }
        // A residual block, and its cells in the last pass, at its price.
        None if tile.level == FLOOR_LEVEL => LEAF_WIDTH as u64 + residual_prices.of(tile),
        None => divide_bits(tile.level, left != 0),
    }
}

/// The bits a divide at `level` takes itself, its children aside: its
/// leaf bit and, where it may mask, its mask-present bit -- and, if it
/// `leaves_any` child to the binding above, its flip bit and child mask.
pub fn divide_bits(level: u8, leaves_any: bool) -> u64 {
    let mut bits = LEAF_WIDTH as u64;
    if copy_or_divide_may_mask(level) {
        bits += MASK_PRESENT_WIDTH as u64;
    }
    if leaves_any {
        bits += LEAVING_BITS;
    }
    bits
}
