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
use crate::gct::pyramids::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{cells_in_tile, tiles_across, Tile, CELL_LEVEL};

/// How many resolution tiles a tile holds `size_offset` levels finer:
/// one payload bit each.
fn payload_bits(size_offset: u8) -> u64 {
    let across = tiles_across(size_offset) as u64;
    across * across
}

/// Bits already counted, by tile and by the complex tiles it is nested
/// in: good for as long as nothing under a counted tile changes -- one
/// pass of the complex tiler, whose commits come at its end. One
/// pyramid a nesting -- few nestings occur in a pass, so finding its
/// pyramid is a short search -- holding each tile's bits plus one, `0`
/// not counted yet.
#[derive(Default)]
pub struct CountedBits(
    /// Each nesting's key, and its pyramid of counted bits.
    Vec<(u64, Pyramid)>,
);

/// Enough for any tile's bits, the whole bitmap's included.
const COUNTED_SHAPE: PyramidShape = PyramidShape { coarsest_level: 0, finest_level: CELL_LEVEL, element_bits: 32 };
/// A tile's element before its bits are counted: counted bits are held
/// plus one, so zero is free to mean not yet.
const NOT_COUNTED: u64 = 0;

impl CountedBits {
    /// The pyramid of bits counted under `nesting_key`'s nesting, made
    /// empty the first time it is asked for.
    fn pyramid_for(&mut self, nesting_key: u64) -> &mut Pyramid {
        let at = match self.0.iter().position(|(key, _)| *key == nesting_key) {
            Some(at) => at,
            None => {
                self.0.push((nesting_key, Pyramid::new(COUNTED_SHAPE)));
                self.0.len() - 1
            }
        };
        &mut self.0[at].1
    }
}

/// The bits `tile` costs, nested in `nested`, `bound_above` the value
/// bound above it, and everything under it.
pub fn bits(complex_tiling: &Pyramid, tile: Tile, nested: &mut NestedResolutions, bound_above: bool) -> u64 {
    bits_counted(complex_tiling, tile, nested, bound_above, &mut CountedBits::default())
}

/// The same, reusing and adding to what `counted` holds for the tiles
/// under `tile` -- but never counting `tile` itself in, which may be a
/// complex tile only for as long as it is being scored.
pub fn bits_counted(
    complex_tiling: &Pyramid,
    tile: Tile,
    nested: &mut NestedResolutions,
    bound_above: bool,
    counted: &mut CountedBits,
) -> u64 {
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
                Some(placement) if placement.is_whole_bind() => payload_bits(0),
                _ => cells_in_tile(tile.level),
            };
    }
    mask_bits + match complex_tiling.placed_at(tile) {
        Some(Placement::Bound { masked_children: 0, .. }) => {
            leaf_bind + resolution_width(tile.level) as u64 + payload_bits(0)
        }
        Some(bind @ Placement::Bound { value, .. }) => {
            // Spelled as a divide that masks and flips the value bound above.
            let mut bind_bits = (LEAF_WIDTH + MASK_PRESENT_WIDTH + FLIP_WIDTH) as u64;
            for child in tile.children() {
                bind_bits += MASK_BIT_WIDTH as u64;
                if bind.masks(child) {
                    bind_bits += remembered(complex_tiling, child, nested, value, counted);
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
                for child in tile.children() {
                    copy_bits += MASK_BIT_WIDTH as u64;
                    if copy.masks(child) {
                        copy_bits += remembered(complex_tiling, child, nested, bound_above, counted);
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
                            tile.children()
                                .into_iter()
                                .map(|child| remembered(complex_tiling, child, inside, bound_above, counted))
                                .sum::<u64>()
                        })
                }
            }
            None => {
                let mut divide_bits = LEAF_WIDTH as u64;
                if divide_may_mask(tile.level) {
                    divide_bits += MASK_PRESENT_WIDTH as u64;
                }
                let left: Vec<bool> =
                    tile.children().into_iter().map(|child| complex_tiling.left_to_binding_above(child, bound_above, nested)).collect();
                let leaves_some = left.contains(&true);
                if leaves_some {
                    divide_bits += FLIP_WIDTH as u64;
                }
                for (child, is_left) in tile.children().into_iter().zip(left) {
                    if leaves_some {
                        divide_bits += MASK_BIT_WIDTH as u64;
                    }
                    if !is_left {
                        divide_bits += remembered(complex_tiling, child, nested, bound_above, counted);
                    }
                }
                divide_bits
            }
        },
    }
}

/// `tile`'s bits, from `counted` if counted before, else counted now.
fn remembered(
    complex_tiling: &Pyramid,
    tile: Tile,
    nested: &mut NestedResolutions,
    bound_above: bool,
    counted: &mut CountedBits,
) -> u64 {
    let nesting_key = nested.key();
    let known = counted.pyramid_for(nesting_key).get(tile);
    if known != NOT_COUNTED {
        return known - 1;
    }
    let bits = bits_counted(complex_tiling, tile, nested, bound_above, counted);
    counted.pyramid_for(nesting_key).set(tile, bits + 1);
    bits
}
