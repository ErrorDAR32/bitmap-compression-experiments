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
//! pyramid](super::cost_pyramid). The best size offset saves the most,
//! and a candidate that saves nothing is none.

use super::bit_cost::{bits_with, node_bits};
use super::cost_pyramid::CostPyramid;
use crate::gct::grammar::raw_resolution_fits;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::complex_tiling::{ComplexTiling, Fields};
use crate::gct::tile::{Tile, CELL_LEVEL, FLOOR_LEVEL};
use crate::Bitmap;

/// The smallest tile a complex tile can be: 4x4, so its resolution is
/// at least 2x2.
pub const FINEST_CANDIDATE_LEVEL: u8 = CELL_LEVEL - 2;

/// A tile the complex tiler could make a complex tile, at its best
/// size offset. Every field is bytes, so a list of them has no padding.
#[derive(Clone, Copy, Default)]
pub struct Candidate {
    /// The tile.
    pub tile: Tile,
    /// How many levels finer than the tile its resolution would be.
    pub size_offset: u8,
    /// Whether, of 1x1 resolution, it would say its cells as a cell list.
    pub cell_list: bool,
    /// The resolutions of the complex tiles `tile` is nested in.
    pub nested: NestedResolutions,
}

const _: () = assert!(
    size_of::<Candidate>() == size_of::<Tile>() + size_of::<u8>() + size_of::<bool>() + size_of::<NestedResolutions>(),
    "a candidate has no padding"
);

/// The finest candidates, 16x16 and finer, whose counts debug builds
/// check against the reference count: coarser ones would count too much
/// of the bitmap again to check every one.
const FINEST_CHECKED_LEVEL: u8 = CELL_LEVEL - 4;

/// In debug builds, for a candidate of 16x16 or finer, that `bits` read
/// off the cost pyramid is what the reference count
/// ([`bits`](super::bit_cost::bits)) gives `tile` with `fields` as its
/// fields -- the reference carrying the
/// value bound above down itself, where the cost pyramid reads each
/// tile's own field.
fn debug_assert_matches_reference(
    complex_tiling: &ComplexTiling,
    bitmap: &Bitmap,
    tile: Tile,
    fields: Fields,
    nested: &NestedResolutions,
    bits: u64,
) {
    if cfg!(debug_assertions) && tile.level >= FINEST_CHECKED_LEVEL {
        let reference = node_bits(complex_tiling, bitmap, tile, fields, &mut nested.clone(), fields.bound_above(), &mut |child, fields, inside, bound_above| {
            bits_with(complex_tiling, bitmap, child, fields, inside, bound_above)
        });
        assert_eq!(bits, reference, "{tile:?}: the cost pyramid's count is not the reference count");
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
    let finest = if raw_resolution_fits(level) { CELL_LEVEL } else { FLOOR_LEVEL };
    (level + 1..=finest).filter(move |&resolution| {
        !nested.has_resolution(resolution)
            && (resolution == CELL_LEVEL || here.any_bound_under(resolution))
            && (resolution > level + 1 || here.entirely_bound_at(resolution))
    })
}

impl Candidate {
    /// `tile`'s best size offset to be a complex tile at, if any saves
    /// bits, and the bits it saves, nested in `nested`, every count read
    /// from `costs`. Changes nothing: each size offset is scored as the
    /// complex tile it would be.
    pub fn best_for(
        complex_tiling: &ComplexTiling,
        bitmap: &Bitmap,
        costs: &CostPyramid,
        tile: Tile,
        nested: &NestedResolutions,
    ) -> Option<(Candidate, u64)> {
        let here = complex_tiling.fields(tile);
        let without = costs.without(tile);
        // A size offset's bits: the tile as that complex tile, its
        // children's counts read from its resolution's slot.
        let with = |fields: Fields, resolution: u8| {
            let bits = node_bits(complex_tiling, bitmap, tile, fields, &mut nested.clone(), here.bound_above(), &mut |child, fields, _, _| {
                costs.child_bits(child, fields, resolution)
            });
            debug_assert_matches_reference(complex_tiling, bitmap, tile, fields, nested, bits);
            bits
        };
        debug_assert_matches_reference(complex_tiling, bitmap, tile, here, nested, without);
        let mut best: Option<(Candidate, u64)> = None;
        for resolution in tried_resolutions(here, tile.level, nested) {
            let size_offset = resolution - tile.level;
            let plain = with(here.as_complex_tile(size_offset), resolution);
            // At 1x1, the cells may go as a cell list instead, when
            // strictly cheaper.
            let listed = (resolution == CELL_LEVEL).then(|| with(here.as_cell_list(tile.level), resolution));
            let (with_bits, cell_list) = match listed {
                Some(listed) if listed < plain => (listed, true),
                _ => (plain, false),
            };
            let Some(saving) = without.checked_sub(with_bits).filter(|&saving| saving > 0) else { continue };
            if best.is_none_or(|(_, best_saving)| saving > best_saving) {
                best = Some((Candidate { tile, size_offset, cell_list, nested: *nested }, saving));
            }
        }
        best
    }
}
