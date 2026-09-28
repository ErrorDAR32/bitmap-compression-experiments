//! The complex tiler, the second pass: groups the greedy tiler's placed
//! tiles into complex tiles, nested as deep as they keep paying. Its
//! output is one pyramid, which tiles are complex tiles and at what
//! depth ([`complex_tile_depths`](crate::gct::pyramids::complex_tile_depths)).
//! Never looks at the bitmap: every decision is made from the tiles
//! the greedy tiler placed.
//!
//! One pass per nesting depth. The first pass searches the whole bitmap
//! for the outermost complex tiles, which capture the coarse structure.
//! Each later pass searches only inside the complex tiles the pass
//! before it committed, for complex tiles nested in them -- another
//! resolution for the areas those mask. Passes stop when one commits
//! nothing. Each pass scores every candidate once, sorts them best
//! first, and commits them in that order, skipping any that overlap one
//! already committed in the same pass.

use super::complex_tile_candidates::Candidate;
use crate::gct::pyramids::complex_tile_depths::ComplexTileDepths;
use crate::gct::pyramids::placements::Placements;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{Tile, CELL_LEVEL};
use crate::gct::enclosing::Enclosing;
use crate::Bitmap;

/// Where one pass searches: an area, the coarsest level a candidate in
/// it may be, and the complex tiles enclosing it.
struct SearchArea {
    area: Tile,
    coarsest_level: usize,
    enclosing: Enclosing,
}

/// Which tiles are complex tiles, and at what depth, given what the
/// greedy tiler placed and its bound tile counts.
pub fn complex_tiler(placements: &Pyramid, counts: &Vec<Pyramid>) -> Pyramid {
    let mut depths = Pyramid::complex_tile_depths();

    let mut searched = vec![SearchArea { area: Tile::whole_bitmap(), coarsest_level: 0, enclosing: Enclosing::none() }];
    while !searched.is_empty() {
        let mut candidates = candidates_in(placements, counts, &searched);
        candidates.sort_by(Candidate::best_first);
        searched = commit(candidates, &mut depths);
    }
    depths
}

/// Every candidate in this pass's search areas, biggest tile size
/// first, down to the smallest a complex tile can be (4x4, so its
/// resolution is at least 2x2).
fn candidates_in(placements: &Pyramid, counts: &Vec<Pyramid>, searched: &[SearchArea]) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for search in searched {
        for level in search.coarsest_level..=(CELL_LEVEL - 2) {
            for tile in search.area.tiles_at_depth(level - search.area.level) {
                if placements.is_placed(tile) || search.enclosing.relating(counts, tile).is_some() {
                    continue;
                }
                if let Some(candidate) = Candidate::best_for(counts, tile, &search.enclosing) {
                    candidates.push(candidate);
                }
            }
        }
    }
    candidates
}

/// Commits `candidates`, best first, skipping any overlapping one
/// already committed; returns where the next pass searches -- inside
/// each one committed.
fn commit(candidates: Vec<Candidate>, depths: &mut Pyramid) -> Vec<SearchArea> {
    let mut claimed = Bitmap::new();
    let mut next = Vec::new();
    for candidate in candidates {
        let (left, top, right, bottom) = candidate.tile.cell_rect();
        // The corner alone catches a tile already inside an earlier,
        // coarser-or-equal commitment; the full rect is still needed the
        // other way round, when this one would contain something smaller
        // already committed.
        if claimed.get(left, top) || claimed.any_set_in_rect(left, top, right, bottom) {
            continue;
        }
        claimed.set_rect(left as i64, top as i64, right as i64, bottom as i64);
        depths.set_complex_tile_depth(candidate.tile, candidate.depth);
        next.push(SearchArea {
            area: candidate.tile,
            coarsest_level: candidate.tile.level + 1,
            enclosing: candidate.enclosing.with(candidate.tile.level + candidate.depth),
        });
    }
    next
}
