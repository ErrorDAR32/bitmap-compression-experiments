//! A candidate complex tile: a tile, the resolution it would be said at,
//! what that is worth, and how candidates rank against each other.
//!
//! A candidate is a tile nothing is placed at directly, not already
//! entirely related to a complex tile enclosing it, tried at every depth
//! from `1` to the 2x2 floor (a 1x1 resolution is never tried: 1x1 tiles
//! are always the residual pass's own). A depth whose resolution an
//! enclosing complex tile already has is skipped: those tiles are
//! already related to it.
//!
//! - `unmasked_cells`: the cells the `Bound` tiles placed at exactly
//!   that resolution cover.
//! - `total_cells`: the tile's cells, minus what is already related to
//!   an enclosing complex tile -- what that one says costs the
//!   candidate nothing, so it does not count against it either.
//!
//! Depth `1` must have all four children unmasked (masking never pays
//! there); deeper, `4 * unmasked_cells >= 3 * total_cells`.
//! `total_cells` is the same at every depth, so the best depth is the
//! one with the most unmasked cells, ties toward the coarser.

use crate::gct::pyramids::bound_tile_counts::BoundTileCounts;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{cells_in_tile, Tile, CELL_LEVEL};
use crate::gct::enclosing::Enclosing;
use std::cmp::Ordering;

pub struct Candidate {
    pub tile: Tile,
    pub depth: usize,
    unmasked_cells: u64,
    total_cells: u64,
    /// The complex tiles enclosing `tile`.
    pub enclosing: Enclosing,
}

impl Candidate {
    /// `tile`'s best depth to be a complex tile at, if any.
    pub fn best_for(counts: &Vec<Pyramid>, tile: Tile, enclosing: &Enclosing) -> Option<Candidate> {
        let related_further_out: u64 = enclosing
            .able_to_relate(tile)
            .map(|nesting| enclosing.resolution(nesting))
            .map(|resolution| counts.under(tile, resolution) as u64 * cells_in_tile(resolution))
            .sum();
        let total_cells = cells_in_tile(tile.level) - related_further_out;
        let max_depth = CELL_LEVEL - 1 - tile.level; // 1x1 is never a resolution
        let mut best: Option<Candidate> = None;
        for depth in 1..=max_depth {
            let resolution = tile.level + depth;
            if enclosing.has_resolution(resolution) {
                continue; // those tiles are already related to that complex tile
            }
            let unmasked = counts.under(tile, resolution);
            if unmasked == 0 || (depth == 1 && unmasked != 4) {
                continue; // nothing to gain, or masking at depth 1, which never pays
            }
            let unmasked_cells = unmasked as u64 * cells_in_tile(resolution);
            if 4 * unmasked_cells < 3 * total_cells {
                continue; // below the floor
            }
            if best.as_ref().is_none_or(|current| unmasked_cells > current.unmasked_cells) {
                best = Some(Candidate { tile, depth, unmasked_cells, total_cells, enclosing: enclosing.clone() });
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
