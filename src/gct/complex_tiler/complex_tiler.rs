//! The complex tiler, the second pass: groups the greedy tiler's placed
//! tiles into complex tiles, nested as deep as they keep paying. Its
//! output is one pyramid, which tiles are complex tiles and at what
//! size offset ([`complex_tile_size_offsets`](crate::gct::pyramids::complex_tile_size_offsets)).
//! Never looks at the bitmap: every decision is made from the tiles
//! the greedy tiler placed.
//!
//! One pass per nesting level. The first pass searches the whole bitmap
//! for the outermost complex tiles, which capture the coarse structure.
//! Each later pass searches only inside the complex tiles the pass
//! before it committed, for complex tiles nested in them -- another
//! resolution for the areas those mask. Passes stop when one commits
//! nothing. Each pass scores every candidate once, sorts them best
//! first, and commits them in that order, skipping any that overlap one
//! already committed in the same pass.

use super::complex_tile_candidates::Candidate;
use crate::gct::pyramids::complex_tile_size_offsets::ComplexTileSizeOffsets;
use crate::gct::pyramids::placements::Placements;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{Tile, CELL_LEVEL};
use crate::gct::nested_resolutions::NestedResolutions;
use crate::Bitmap;

/// Where one pass searches: an area, the coarsest level a candidate in
/// it may be, and the resolutions of the complex tiles it is nested in.
struct SearchArea {
    area: Tile,
    coarsest_level: u8,
    nested: NestedResolutions,
}

/// Creates complex tiles from the greedy tiler's output: which tiles
/// are complex tiles, and at what size offset, given what the greedy tiler
/// placed and its bound tile counts.
pub fn complex_tiler(placements: &Pyramid, bound_tile_counts: &Vec<Pyramid>) -> Pyramid {
    let mut size_offsets = Pyramid::complex_tile_size_offsets();

    let mut searched = vec![SearchArea { area: Tile::whole_bitmap(), coarsest_level: 0, nested: NestedResolutions::none() }];
    while !searched.is_empty() {
        let mut candidates = candidates_in(placements, bound_tile_counts, &searched);
        candidates.sort_by(Candidate::best_first);
        searched = commit(candidates, &mut size_offsets);
    }
    size_offsets
}

/// Every candidate in this pass's search areas, biggest tile size
/// first, down to the smallest a complex tile can be (4x4, so its
/// resolution is at least 2x2).
fn candidates_in(placements: &Pyramid, bound_tile_counts: &Vec<Pyramid>, searched: &[SearchArea]) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for search in searched {
        for level in search.coarsest_level..=(CELL_LEVEL - 2) {
            for tile in search.area.tiles_at_size_offset(level - search.area.level) {
                if placements.is_placed(tile) || search.nested.unmasking(bound_tile_counts, tile).is_some() {
                    continue;
                }
                if let Some(candidate) = Candidate::best_for(bound_tile_counts, tile, &search.nested) {
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
fn commit(candidates: Vec<Candidate>, size_offsets: &mut Pyramid) -> Vec<SearchArea> {
    let mut claimed = Bitmap::new();
    let mut next = Vec::new();
    for candidate in candidates {
        // The corner alone catches a tile already inside an earlier,
        // coarser-or-equal commitment; the full rect is still needed the
        // other way round, when this one would contain something smaller
        // already committed.
        if candidate.tile.top_left_value(&claimed) || candidate.tile.any_set_in(&claimed) {
            continue;
        }
        candidate.tile.set_in(&mut claimed);
        size_offsets.set_complex_tile_size_offset(candidate.tile, candidate.size_offset);
        next.push(SearchArea {
            area: candidate.tile,
            coarsest_level: candidate.tile.level + 1,
            nested: candidate.nested.with_nested(candidate.tile.level + candidate.size_offset),
        });
    }
    next
}
