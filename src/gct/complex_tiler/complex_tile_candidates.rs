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
//! complex tile. The best size offset saves the most, and a candidate
//! that saves nothing is none.

use super::bit_cost::{bits_counted, CountedBits};
use crate::gct::grammar::raw_resolution_fits;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::pyramids::placements::binding_above;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{Tile, CELL_LEVEL};

/// A tile the complex tiler could make a complex tile, at its best
/// size offset, and what that would save.
pub struct Candidate {
    /// The tile.
    pub tile: Tile,
    /// How many levels finer than the tile its resolution would be.
    pub size_offset: u8,
    /// The bits it saves, as the tiling stood when it was counted.
    pub saving: u64,
    /// The resolutions of the complex tiles `tile` is nested in.
    pub nested: NestedResolutions,
}

impl Candidate {
    /// `tile`'s best size offset to be a complex tile at, if any saves
    /// bits. Leaves `complex_tiling` as it found it.
    pub fn best_for(
        complex_tiling: &mut Pyramid,
        tile: Tile,
        nested: &NestedResolutions,
        counted: &mut CountedBits,
    ) -> Option<Candidate> {
        let bound_above = binding_above(tile, |at| complex_tiling.placed_at(at));
        let mut inside = *nested;
        let without = bits_counted(complex_tiling, tile, &mut inside, bound_above, counted);
        let mut best: Option<Candidate> = None;
        let finest = if raw_resolution_fits(tile.level) { CELL_LEVEL } else { CELL_LEVEL - 1 };
        for size_offset in 1..=finest - tile.level {
            let resolution = tile.level + size_offset;
            if nested.has_resolution(resolution) {
                continue; // those tiles are already unmasked in that complex tile
            }
            let nothing_bound = resolution < CELL_LEVEL && !complex_tiling.any_bound_under(tile, resolution);
            if nothing_bound || (size_offset == 1 && !complex_tiling.entirely_bound_at(tile, resolution)) {
                continue; // nothing to unmask, or masking at size offset 1, which the grammar cannot say
            }
            complex_tiling.make_complex_tile(tile, size_offset);
            let with = bits_counted(complex_tiling, tile, &mut inside, bound_above, counted);
            complex_tiling.clear_complex_tile(tile);
            let Some(saving) = without.checked_sub(with).filter(|&saving| saving > 0) else { continue };
            if best.as_ref().is_none_or(|current| saving > current.saving) {
                best = Some(Candidate { tile, size_offset, saving, nested: *nested });
            }
        }
        best
    }
}
