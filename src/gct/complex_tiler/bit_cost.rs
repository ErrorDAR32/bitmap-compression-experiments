//! What a tile costs to say, in bits, as the complex tiling stands: the
//! grammar's own widths ([`crate::gct::grammar`]) applied to what the
//! tiling holds at each tile -- the same rules the tree is read by, so
//! the count is the encoder's own. The complex tiler scores a candidate
//! by the bits it saves: its tile's cost without it, less its cost with
//! it. The one thing not counted is what is not decided yet: the complex
//! tiles later passes will nest inside it.
//!
//! Every check in `tests/common` holds this to the encoder's count, on
//! every bitmap tested.

use crate::gct::grammar::*;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::pyramids::placements::Placement;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{cells_in_tile, tiles_across, Tile, CELL_LEVEL};

/// How many resolution tiles a tile holds `size_offset` levels finer:
/// one payload bit each.
fn payload_bits(size_offset: u8) -> u64 {
    let across = tiles_across(size_offset) as u64;
    across * across
}

/// The bits `tile` costs, nested in `nested`, and everything under it.
pub fn bits(complex_tiling: &Pyramid, tile: Tile, nested: &mut NestedResolutions) -> u64 {
    // Its mask bits, nearest complex tile first, up to the first it is
    // unmasked in -- then its values are that one's payload.
    let mut mask_bits = 0;
    let able_to_unmask: Vec<u8> = nested.able_to_unmask(tile).collect();
    for nesting in able_to_unmask {
        mask_bits += MASK_BIT_WIDTH as u64;
        let resolution = nested.resolution(nesting);
        if complex_tiling.entirely_bound_at(tile, resolution) {
            return mask_bits + payload_bits(resolution - tile.level);
        }
    }

    let leaf_bind = (LEAF_WIDTH + CODE_WIDTH) as u64;
    if tile.level == CELL_LEVEL - 1 {
        // The 2x2 floor: a tile and its value, or a residual and its cells
        // in the residual pass.
        return mask_bits
            + LEAF_WIDTH as u64
            + match complex_tiling.placed_at(tile) {
                Some(Placement::Bound(_)) => payload_bits(0),
                _ => cells_in_tile(tile.level),
            };
    }
    mask_bits + match complex_tiling.placed_at(tile) {
        Some(Placement::Bound(_)) => leaf_bind + resolution_width(tile.level) as u64 + payload_bits(0),
        Some(copy @ Placement::Copied { .. }) => {
            let mut copy_bits = leaf_bind + (FAR_WIDTH + DIRECTION_WIDTH) as u64;
            if copy_may_mask(tile.level) {
                copy_bits += MASK_PRESENT_WIDTH as u64;
            }
            if copy.masks_any() {
                for child in tile.children() {
                    copy_bits += MASK_BIT_WIDTH as u64;
                    if copy.masks(child) {
                        copy_bits += bits(complex_tiling, child, nested);
                    }
                }
            }
            copy_bits
        }
        None => match complex_tiling.complex_tile_size_offset(tile) {
            Some(size_offset) => {
                let mut complex_bits = leaf_bind + resolution_width(tile.level) as u64;
                if complex_tile_may_mask(tile.level, size_offset) {
                    complex_bits += MASK_PRESENT_WIDTH as u64;
                }
                let resolution = tile.level + size_offset;
                if complex_tiling.entirely_bound_at(tile, resolution) {
                    complex_bits + payload_bits(size_offset)
                } else {
                    complex_bits
                        + nested.while_nested(resolution, |inside| {
                            tile.children().into_iter().map(|child| bits(complex_tiling, child, inside)).sum::<u64>()
                        })
                }
            }
            None => LEAF_WIDTH as u64 + tile.children().into_iter().map(|child| bits(complex_tiling, child, nested)).sum::<u64>(),
        },
    }
}
