//! What a tile costs to say, in bits, as the complex tiling stands: the
//! grammar's own widths ([`crate::gct::grammar`]) applied to what the
//! tiling holds at each tile -- the same rules the tree is read by, so
//! the count is the encoder's own. The complex tiler scores a candidate
//! by the bits it saves: its tile's cost without it, less its cost with
//! it. The one thing not counted is what is not decided yet: the complex
//! tiles later passes will nest inside it.
//!
//! Every check in `tests/common` holds this to the encoder's count
//! (`crate::diagnostics::examination` gathers both), on every bitmap
//! tested.

use crate::gct::grammar::point_list;
use crate::gct::grammar::*;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::complex_tiling::{ComplexTiling, Fields};
use crate::gct::pyramids::placements::Placement;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{cells_in_tile, tiles_across, Tile, CELL_LEVEL};
use crate::Bitmap;

/// How many resolution tiles a tile holds `size_offset` levels finer:
/// one payload bit each.
pub fn payload_bits(size_offset: u8) -> u64 {
    let across = tiles_across(size_offset) as u64;
    across * across
}

/// The bits `tile` costs, nested in `nested`, `bound_above` the value
/// bound above it, and everything under it, as the tiling stands: the
/// reference count, each tile counted once, which the tests hold to the
/// encoder's.
pub fn bits(complex_tiling: &Pyramid, bitmap: &Bitmap, tile: Tile, nested: &mut NestedResolutions, bound_above: bool) -> u64 {
    bits_with(complex_tiling, bitmap, tile, complex_tiling.fields(tile), nested, bound_above)
}

/// [`bits`], with `here` as `tile`'s fields.
pub fn bits_with(
    complex_tiling: &Pyramid,
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
    complex_tiling: &Pyramid,
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
    if tile.level == CELL_LEVEL - 1 {
        // The 2x2 floor: a tile and its value, or a residual and its cells
        // in the residual pass.
        return mask_bits
            + LEAF_WIDTH as u64
            + match here.placed() {
                Some(placement) if placement.is_whole_bind() => payload_bits(0),
                _ => cells_in_tile(tile.level),
            };
    }
    mask_bits + match here.placed() {
        Some(Placement::Bound { masked_children: 0, .. }) => {
            leaf_bind + resolution_width(tile.level) as u64 + payload_bits(0)
        }
        Some(bind @ Placement::Bound { value, .. }) => {
            // Spelled as a divide that masks and flips the value bound above.
            let mut bind_bits = (LEAF_WIDTH + MASK_PRESENT_WIDTH + FLIP_WIDTH) as u64;
            for (child, fields) in tile.children().into_iter().zip(complex_tiling.children_fields(tile)) {
                bind_bits += MASK_BIT_WIDTH as u64;
                if bind.masks(child) {
                    bind_bits += child_bits(child, fields, nested, value);
                }
            }
            bind_bits
        }
        Some(copy @ Placement::Copied { .. }) => {
            let mut copy_bits = leaf_bind + (FAR_WIDTH + DIRECTION_WIDTH) as u64;
            if copy_may_mask(tile.level) {
                copy_bits += MASK_PRESENT_WIDTH as u64;
            }
            if copy.masks_any() {
                for (child, fields) in tile.children().into_iter().zip(complex_tiling.children_fields(tile)) {
                    copy_bits += MASK_BIT_WIDTH as u64;
                    if copy.masks(child) {
                        copy_bits += child_bits(child, fields, nested, bound_above);
                    }
                }
            }
            copy_bits
        }
        None => match here.complex_tile_size_offset() {
            Some(size_offset) => {
                let mut complex_bits = leaf_bind + resolution_width(tile.level) as u64;
                if complex_tile_may_mask(tile.level, size_offset) {
                    complex_bits += MASK_PRESENT_WIDTH as u64;
                }
                let resolution = tile.level + size_offset;
                if here.is_point_list() {
                    complex_bits + PAYLOAD_MODE_WIDTH as u64 + point_list::bits(bitmap, tile)
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
            None => {
                let mut divide_bits = LEAF_WIDTH as u64;
                if divide_may_mask(tile.level) {
                    divide_bits += MASK_PRESENT_WIDTH as u64;
                }
                let (children, fields) = (tile.children(), complex_tiling.children_fields(tile));
                let mut left = [false; 4];
                for at in 0..children.len() {
                    left[at] = fields[at].left_to_binding_above(children[at], bound_above, nested);
                }
                let leaves_some = left.contains(&true);
                if leaves_some {
                    divide_bits += FLIP_WIDTH as u64;
                }
                for at in 0..children.len() {
                    if leaves_some {
                        divide_bits += MASK_BIT_WIDTH as u64;
                    }
                    if !left[at] {
                        divide_bits += child_bits(children[at], fields[at], nested, bound_above);
                    }
                }
                divide_bits
            }
        },
    }
}
