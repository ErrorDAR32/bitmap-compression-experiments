//! What a tile costs to say, in bits, as the complex tiling stands: the
//! grammar's own widths ([`crate::gct::grammar`]) applied to what the
//! tiling holds at each tile -- the same rules the tree is read by, so
//! the count is the encoder's own, for any tiling: the reference the
//! complex tiler's own counts ([`super::search`]) are held to in debug
//! builds, and what the tree is counted by. One thing is counted at a
//! stand-in: a residual block's cells, at a bit a cell, as if raw. The
//! last pass codes them from the cells around them, so what they take
//! depends on the whole pass, and is known only by coding it.
//!
//! Every check in `tests/common` holds this to the encoder's count
//! (`crate::diagnostics::examination` gathers both), on every bitmap
//! tested.

use crate::gct::grammar::cell_list;
use crate::gct::grammar::*;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::complex_tiling::{ComplexTiling, Fields};
use crate::gct::pyramids::placements::Placement;
use crate::gct::tile::{tiles_in_level, Tile, CELL_LEVEL, CHILDREN, FLOOR_LEVEL};
use crate::Bitmap;

/// How many resolution tiles a tile holds `size_offset` levels finer:
/// one payload bit each.
pub fn payload_bits(size_offset: u8) -> u64 {
    tiles_in_level(size_offset) as u64
}

/// The bits `tile` costs, nested in `nested`, `bound_above` the value
/// bound above it, and everything under it, as the tiling stands: the
/// reference count, each tile counted once, which the tests hold to the
/// encoder's.
pub fn bits(complex_tiling: &ComplexTiling, bitmap: &Bitmap, tile: Tile, nested: &mut NestedResolutions, bound_above: bool) -> u64 {
    bits_with(complex_tiling, bitmap, tile, complex_tiling.fields(tile), nested, bound_above)
}

/// The bits a cell list at a tile of `level` spends before its list,
/// its mask bits aside: its leaf and code bits, its size offset, its
/// mask-present bit and its payload mode.
pub fn cell_list_header_bits(level: u8) -> u64 {
    let mut bits = (LEAF_WIDTH + CODE_WIDTH) as u64 + resolution_width(level) as u64 + PAYLOAD_MODE_WIDTH as u64;
    if complex_tile_may_mask(CELL_LEVEL - level) {
        bits += MASK_PRESENT_WIDTH as u64;
    }
    bits
}

/// The bits the whole tree takes, as the tiling stands, starting at
/// `start_level`: the start level, then every tile of that level --
/// residual blocks at a bit a cell.
pub fn tree_bits(complex_tiling: &ComplexTiling, bitmap: &Bitmap, start_level: u8) -> u64 {
    START_LEVEL_WIDTH as u64
        + Tile::all_of_level(start_level)
            .map(|tile| bits(complex_tiling, bitmap, tile, &mut NestedResolutions::none(), BOUND_AT_THE_TOP))
            .sum::<u64>()
}

/// The bits the tree takes that says every cell in a cell list: its
/// start level the coarsest a cell list can be named at, and every tile
/// of that level a cell list.
pub fn cell_lists_tree_bits(complex_tiling: &ComplexTiling, bitmap: &Bitmap) -> u64 {
    START_LEVEL_WIDTH as u64
        + Tile::all_of_level(COARSEST_RAW_LEVEL)
            .map(|tile| {
                let listed = complex_tiling.fields(tile).as_cell_list(tile.level);
                bits_with(complex_tiling, bitmap, tile, listed, &mut NestedResolutions::none(), BOUND_AT_THE_TOP)
            })
            .sum::<u64>()
}

/// [`bits`], with `here` as `tile`'s fields.
pub fn bits_with(
    complex_tiling: &ComplexTiling,
    bitmap: &Bitmap,
    tile: Tile,
    here: Fields,
    nested: &mut NestedResolutions,
    bound_above: bool,
) -> u64 {
    node_bits(complex_tiling, bitmap, tile, here, nested, bound_above, &mut |child, fields, nested, bound_above| {
        bits_with(complex_tiling, bitmap, child, fields, nested, bound_above)
    })
}

/// The bits `tile` costs with `here` as its fields -- which need not be
/// the tiling's: a candidate is scored as the complex tile it would be,
/// with nothing changed -- nested in `nested`, `bound_above` the value
/// bound above it: its own bits, and each child node's as
/// `child_bits(child, its fields, nested, bound_above)` gives them --
/// the four children's fields read together, once.
pub fn node_bits(
    complex_tiling: &ComplexTiling,
    bitmap: &Bitmap,
    tile: Tile,
    here: Fields,
    nested: &mut NestedResolutions,
    bound_above: bool,
    child_bits: &mut impl FnMut(Tile, Fields, &mut NestedResolutions, bool) -> u64,
) -> u64 {
    // Its mask bits, nearest complex tile first, up to the first it is
    // unmasked in -- then its values are that one's payload.
    let mut mask_bits = 0;
    for nesting in nested.able_to_unmask(tile) {
        mask_bits += MASK_BIT_WIDTH as u64;
        let resolution = nested.resolution(nesting);
        if here.entirely_bound_at(resolution) {
            return mask_bits + payload_bits(resolution - tile.level);
        }
    }

    let leaf_bind = (LEAF_WIDTH + CODE_WIDTH) as u64;
    mask_bits + match here.placed() {
        Some(Placement::Bound { masked_children: 0, .. }) => {
            leaf_bind + bind_resolution_width(tile.level, nested.in_body()) as u64 + payload_bits(0)
        }
        Some(bind @ Placement::Bound { .. }) => {
            // Spelled as a divide that masks and flips the value bound above.
            MASKING_DIVIDE_HEADER_WIDTH as u64 + masked_children_bits(complex_tiling, tile, bind, nested, bound_above, child_bits)
        }
        Some(copy @ Placement::Copied { .. }) => {
            let mut copy_bits = leaf_bind + (FAR_WIDTH + DIRECTION_WIDTH) as u64;
            if copy_or_divide_may_mask(tile.level) {
                copy_bits += MASK_PRESENT_WIDTH as u64;
            }
            if copy.masks_any() {
                copy_bits += masked_children_bits(complex_tiling, tile, copy, nested, bound_above, child_bits);
            }
            copy_bits
        }
        None => match here.complex_tile_size_offset() {
            Some(size_offset) => {
                let mut complex_bits = leaf_bind + resolution_width(tile.level) as u64;
                if complex_tile_may_mask(size_offset) {
                    complex_bits += MASK_PRESENT_WIDTH as u64;
                }
                let resolution = tile.level + size_offset;
                if here.is_cell_list() {
                    complex_bits + PAYLOAD_MODE_WIDTH as u64 + cell_list::bits(bitmap, tile)
                } else if here.entirely_bound_at(resolution) {
                    if has_payload_mode(tile.level, size_offset, false) {
                        complex_bits += PAYLOAD_MODE_WIDTH as u64;
                    }
                    complex_bits + payload_bits(size_offset)
                } else {
                    complex_bits
                        + nested.while_nested(resolution, |inside| {
                            tile.children()
                                .into_iter()
                                .zip(complex_tiling.children_fields(tile))
                                .map(|(child, fields)| child_bits(child, fields, inside, bound_above))
                                .sum::<u64>()
                        })
                }
            }
            None if tile.level == FLOOR_LEVEL => {
                // A residual block, and its cells in the last pass.
                (LEAF_WIDTH + RESIDUAL_BLOCK_BITS) as u64
            }
            None => {
                let mut divide_bits = LEAF_WIDTH as u64;
                if copy_or_divide_may_mask(tile.level) {
                    divide_bits += MASK_PRESENT_WIDTH as u64;
                }
                let (children, fields) = (tile.children(), complex_tiling.children_fields(tile));
                let left: [bool; CHILDREN as usize] =
                    std::array::from_fn(|child_index| fields[child_index].left_to_binding_above(children[child_index], bound_above, nested));
                if left.contains(&true) {
                    // A divide that masks, keeping the value bound above.
                    divide_bits += (FLIP_WIDTH + CHILD_MASK_WIDTH) as u64;
                }
                for child_index in 0..children.len() {
                    if !left[child_index] {
                        divide_bits += child_bits(children[child_index], fields[child_index], nested, bound_above);
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
    nested: &mut NestedResolutions,
    bound_above: bool,
    child_bits: &mut impl FnMut(Tile, Fields, &mut NestedResolutions, bool) -> u64,
) -> u64 {
    let bound_inside = placed.bound_inside(bound_above);
    let mut bits = CHILD_MASK_WIDTH as u64;
    for (child, fields) in tile.children().into_iter().zip(complex_tiling.children_fields(tile)) {
        if placed.masks(child) {
            bits += child_bits(child, fields, nested, bound_inside);
        }
    }
    bits
}
