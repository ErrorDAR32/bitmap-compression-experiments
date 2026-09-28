//! A candidate complex tile: a tile, the resolution it would be said at,
//! what that is worth, and how candidates rank against each other.
//!
//! A candidate is a tile nothing is placed at directly, not already
//! entirely unmasked in a complex tile it is nested in, tried at every size offset
//! from `1` to the 2x2 floor (a 1x1 resolution is never tried: 1x1 tiles
//! are always the residual pass's own). A size offset whose resolution an
//! complex tile it is nested in already has is skipped: those tiles
//! are already unmasked in it.
//!
//! - `unmasked_cells`: the cells the `Bound` tiles placed at exactly
//!   that resolution cover.
//! - `total_cells`: the tile's cells, minus what is already unmasked in
//!   a complex tile it is nested in -- what that one binds costs the
//!   candidate nothing, so it does not count against it either.
//!
//! Depth `1` must have all four children unmasked (masking never pays
//! there); deeper, `4 * unmasked_cells >= 3 * total_cells`.
//! `total_cells` is the same at every size offset, so the best one is the
//! one with the most unmasked cells, ties toward the coarser.

use crate::gct::pyramids::bound_tile_counts::BoundTileCounts;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{cells_in_tile, Tile, CELL_LEVEL};
use crate::gct::nested_resolutions::NestedResolutions;
use std::cmp::Ordering;

pub struct Candidate {
    pub tile: Tile,
    pub size_offset: u8,
    unmasked_cells: u64,
    total_cells: u64,
    /// The resolutions of the complex tiles `tile` is nested in.
    pub nested: NestedResolutions,
}

impl Candidate {
    /// `tile`'s best size offset to be a complex tile at, if any.
    pub fn best_for(bound_tile_counts: &Vec<Pyramid>, tile: Tile, nested: &NestedResolutions) -> Option<Candidate> {
        let unmasked_in_outer: u64 = nested
            .able_to_unmask(tile)
            .map(|nesting| nested.resolution(nesting))
            .map(|resolution| bound_tile_counts.under(tile, resolution) as u64 * cells_in_tile(resolution))
            .sum();
        let total_cells = cells_in_tile(tile.level) - unmasked_in_outer;
        let max_size_offset = CELL_LEVEL - 1 - tile.level; // 1x1 is never a resolution
        let mut best: Option<Candidate> = None;
        for size_offset in 1..=max_size_offset {
            let resolution = tile.level + size_offset;
            if nested.has_resolution(resolution) {
                continue; // those tiles are already unmasked in that complex tile
            }
            let unmasked = bound_tile_counts.under(tile, resolution);
            if unmasked == 0 || (size_offset == 1 && unmasked != 4) {
                continue; // nothing to gain, or masking at size_offset 1, which never pays
            }
            let unmasked_cells = unmasked as u64 * cells_in_tile(resolution);
            if 4 * unmasked_cells < 3 * total_cells {
                continue; // below the floor
            }
            if best.as_ref().is_none_or(|current| unmasked_cells > current.unmasked_cells) {
                best = Some(Candidate { tile, size_offset, unmasked_cells, total_cells, nested: nested.clone() });
            }
        }
        best
    }

    /// Best first: more unmasked cells, then better ratio, then bigger
    /// tile, then reading order. Ranking by ratio first would let a
    /// small, ratio-perfect tile always pre-empt a bigger one that needs
    /// a little masking -- measured on this codebase: a reclaim
    /// capability ranked that way never once won on the sample corpus.
    pub fn best_first(a: &Candidate, b: &Candidate) -> Ordering {
        b.unmasked_cells
            .cmp(&a.unmasked_cells)
            .then_with(|| (b.unmasked_cells * a.total_cells).cmp(&(a.unmasked_cells * b.total_cells)))
            .then_with(|| a.tile.level.cmp(&b.tile.level))
            .then_with(|| (a.tile.y, a.tile.x).cmp(&(b.tile.y, b.tile.x)))
    }
}
