//! A candidate complex tile: a tile, the size offset it would be one
//! at, and the bits that saves.
//!
//! A candidate is a tile nothing is placed at directly and not already
//! entirely unmasked in a complex tile it is nested in, tried at every
//! size offset from `1` to a 2x2 resolution, and at 1x1 -- every cell
//! raw -- where the grammar can name it. Skipped: a resolution a
//! complex tile it is nested in already has, one coarser than 1x1 no
//! `Bound` tile under it is placed at, and size offset `1` unless all
//! four children are bound at it -- the grammar gives size offset `1` no
//! way to mask.
//!
//! What a size offset is worth is counted, not guessed: the tile's
//! [bits](super::bit_cost) as the tiling stands, less its bits as that
//! complex tile -- its children's read from the [cost
//! lanes](super::cost_lanes). The best size offset saves the most, and a candidate
//! that saves nothing is none.

use super::bit_cost::node_bits;
use super::cost_lanes::CostLanes;
use crate::gct::grammar::raw_resolution_fits;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::complex_tiling::{ComplexTiling, Fields};
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{Tile, CELL_LEVEL};
use crate::Bitmap;

/// A tile the complex tiler could make a complex tile, at its best
/// size offset, and what that would save.
#[derive(Clone, Copy, Default)]
pub struct Candidate {
    /// The tile.
    pub tile: Tile,
    /// How many levels finer than the tile its resolution would be.
    pub size_offset: u8,
    /// Whether, of 1x1 resolution, it would say its cells as a point list.
    pub point_list: bool,
    /// The bits it saves, as the tiling stood when it was counted.
    pub saving: u64,
    /// The resolutions of the complex tiles `tile` is nested in.
    pub nested: NestedResolutions,
}

/// The finest candidates, 16x16 and finer, whose counts debug builds
/// check against the reference count: coarser ones would count too much
/// of the bitmap again to check every one.
const FINEST_CHECKED_LEVEL: u8 = CELL_LEVEL - 4;

/// In debug builds, for a candidate of 16x16 or finer, that `bits` read
/// off the lanes is what the reference count ([`bits`]) gives `tile`
/// with `fields` as its fields.
fn debug_assert_matches_reference(
    complex_tiling: &Pyramid,
    bitmap: &Bitmap,
    tile: Tile,
    fields: Fields,
    nested: &NestedResolutions,
    bound_above: bool,
    bits: u64,
) {
    if cfg!(debug_assertions) && tile.level >= FINEST_CHECKED_LEVEL {
        let reference = node_bits(complex_tiling, bitmap, tile, fields, &mut nested.clone(), bound_above, &mut |child, inside, bound_above| {
            super::bit_cost::bits(complex_tiling, bitmap, child, inside, bound_above)
        });
        assert_eq!(bits, reference, "{tile:?}: the lanes' count is not the reference count");
    }
}

/// The resolutions a candidate at a tile of `level`, whose fields are
/// `here`, nested in `nested`, is tried at, coarsest first: from one
/// level finer to 2x2, and 1x1 where the grammar can name it -- but not
/// one a complex tile it is nested in already has (those tiles are
/// already unmasked in it), one coarser than 1x1 no whole bind under it
/// is placed at (nothing to unmask), nor one level finer unless it is
/// entirely bound there (the grammar gives size offset `1` no way to
/// mask).
pub fn tried_resolutions(here: Fields, level: u8, nested: &NestedResolutions) -> impl Iterator<Item = u8> + '_ {
    let finest = if raw_resolution_fits(level) { CELL_LEVEL } else { CELL_LEVEL - 1 };
    (level + 1..=finest).filter(move |&resolution| {
        !nested.has_resolution(resolution)
            && (resolution == CELL_LEVEL || here.any_bound_under(resolution))
            && (resolution > level + 1 || here.entirely_bound_at(resolution))
    })
}

impl Candidate {
    /// `tile`'s best size offset to be a complex tile at, if any saves
    /// bits, nested in `nested`, `bound_above` the value bound above it,
    /// every count read from `lanes`. Changes nothing: each size offset
    /// is scored as the complex tile it would be.
    pub fn best_for(
        complex_tiling: &Pyramid,
        bitmap: &Bitmap,
        lanes: &CostLanes,
        tile: Tile,
        nested: &NestedResolutions,
        bound_above: bool,
    ) -> Option<Candidate> {
        let here = complex_tiling.fields(tile);
        let without = lanes.without(tile);
        // A size offset's bits: the tile as that complex tile, its
        // children read from the lane of its resolution.
        let with = |fields: Fields, resolution: u8| {
            let bits = node_bits(complex_tiling, bitmap, tile, fields, &mut nested.clone(), bound_above, &mut |child, inside, bound_above| {
                lanes.child_bits(complex_tiling, bitmap, child, inside, bound_above, resolution)
            });
            debug_assert_matches_reference(complex_tiling, bitmap, tile, fields, nested, bound_above, bits);
            bits
        };
        debug_assert_matches_reference(complex_tiling, bitmap, tile, here, nested, bound_above, without);
        let mut best: Option<Candidate> = None;
        for resolution in tried_resolutions(here, tile.level, nested) {
            let size_offset = resolution - tile.level;
            let plain = with(here.as_complex_tile(size_offset), resolution);
            // At 1x1, the cells may go as a point list instead, when
            // strictly cheaper.
            let mut with_bits = (plain, false);
            if resolution == CELL_LEVEL {
                let listed = with(here.as_point_list(tile.level), resolution);
                if listed < plain {
                    with_bits = (listed, true);
                }
            }
            let (with_bits, point_list) = with_bits;
            let Some(saving) = without.checked_sub(with_bits).filter(|&saving| saving > 0) else { continue };
            if best.as_ref().is_none_or(|current| saving > current.saving) {
                best = Some(Candidate { tile, size_offset, point_list, saving, nested: *nested });
            }
        }
        best
    }
}
