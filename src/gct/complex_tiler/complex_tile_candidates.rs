//! A candidate complex tile: a tile, the size offset it would be one
//! at, and the bits that saves.
//!
//! A candidate is a tile nothing is placed at directly and not already
//! entirely unmasked in a complex tile it is nested in, tried at every
//! size offset from `1` to the 2x2 floor (a 1x1 resolution is never
//! tried: 1x1 tiles are always the residual pass's own). Skipped: a
//! resolution a complex tile it is nested in already has, one no
//! `Bound` tile under it is placed at, and size offset `1` unless all
//! four children are bound at it -- the grammar gives size offset `1` no
//! way to mask.
//!
//! What a size offset is worth is counted, not guessed: the tile's
//! [bits](super::bit_cost) as the tiling stands, less its bits as that
//! complex tile. The best size offset saves the most, and a candidate
//! that saves nothing is none.

use super::bit_cost::bits;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::bound_tiles_per_level::BoundTilesPerLevel;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{Tile, CELL_LEVEL};

pub struct Candidate {
    pub tile: Tile,
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
        bound_tiles_per_level: &Vec<Pyramid>,
        tile: Tile,
        nested: &NestedResolutions,
    ) -> Option<Candidate> {
        let mut inside = nested.clone();
        let without = bits(complex_tiling, tile, &mut inside);
        let max_size_offset = CELL_LEVEL - 1 - tile.level; // 1x1 is never a resolution
        let mut best: Option<Candidate> = None;
        for size_offset in 1..=max_size_offset {
            let resolution = tile.level + size_offset;
            if nested.has_resolution(resolution) {
                continue; // those tiles are already unmasked in that complex tile
            }
            let bound = bound_tiles_per_level.under(tile, resolution);
            if bound == 0 || (size_offset == 1 && !complex_tiling.entirely_bound_at(tile, resolution)) {
                continue; // nothing to unmask, or masking at size offset 1, which the grammar cannot say
            }
            complex_tiling.make_complex_tile(tile, size_offset);
            let with = bits(complex_tiling, tile, &mut inside);
            complex_tiling.clear_complex_tile(tile);
            let Some(saving) = without.checked_sub(with).filter(|&saving| saving > 0) else { continue };
            if best.as_ref().is_none_or(|current| saving > current.saving) {
                best = Some(Candidate { tile, size_offset, saving, nested: nested.clone() });
            }
        }
        best
    }
}
