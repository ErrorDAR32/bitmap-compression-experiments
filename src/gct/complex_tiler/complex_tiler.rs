//! The complex tiler, the second pass: groups the greedy tiler's placed
//! tiles into complex tiles, nested as deep as they keep paying. Its
//! output is one pyramid, the
//! [`complex_tiling`](crate::gct::pyramids::complex_tiling): the
//! placements, each tile's single bound size, and which tiles are
//! complex tiles at what size offset -- all the tree is read from.
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
use crate::gct::pyramids::bound_tiles_per_level::BoundTilesPerLevel;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
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

/// Creates complex tiles from the greedy tiler's output (its
/// placements pyramid), and returns the complex tiling: the placements,
/// with every committed complex tile's size offset added.
pub fn complex_tiler(placements: &Pyramid) -> Pyramid {
    let bound_tiles_per_level = Vec::<Pyramid>::bound_tiles_per_level(placements);
    let mut complex_tiling = Pyramid::complex_tiling(placements);

    let mut searched = vec![SearchArea { area: Tile::whole_bitmap(), coarsest_level: 0, nested: NestedResolutions::none() }];
    while !searched.is_empty() {
        let mut candidates = candidates_in(&complex_tiling, &bound_tiles_per_level, &searched);
        candidates.sort_by(Candidate::best_first);
        searched = commit(candidates, &mut complex_tiling);
    }
    complex_tiling
}

/// Every candidate in this pass's search areas, biggest tile size
/// first, down to the smallest a complex tile can be (4x4, so its
/// resolution is at least 2x2).
fn candidates_in(complex_tiling: &Pyramid, bound_tiles_per_level: &Vec<Pyramid>, searched: &[SearchArea]) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for search in searched {
        for level in search.coarsest_level..=(CELL_LEVEL - 2) {
            for tile in search.area.tiles_at_size_offset(level - search.area.level) {
                if complex_tiling.placed_at(tile).is_some() || search.nested.unmasking(complex_tiling, tile).is_some() {
                    continue;
                }
                if let Some(candidate) = Candidate::best_for(bound_tiles_per_level, tile, &search.nested) {
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
fn commit(candidates: Vec<Candidate>, complex_tiling: &mut Pyramid) -> Vec<SearchArea> {
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
        complex_tiling.make_complex_tile(candidate.tile, candidate.size_offset);
        next.push(SearchArea {
            area: candidate.tile,
            coarsest_level: candidate.tile.level + 1,
            nested: candidate.nested.with_nested(candidate.tile.level + candidate.size_offset),
        });
    }
    next
}
