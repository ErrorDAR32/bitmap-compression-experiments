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
//! nothing. Each pass commits the candidates that save the most bits
//! between them without overlapping, found bottom-up: a tile keeps its
//! own candidate when that saves at least as much as the best its four
//! children keep between them. Tiles that do not overlap cost bits
//! independently, so that is the best a pass can do.

use super::bit_cost::CountedBits;
use super::complex_tile_candidates::Candidate;
use super::raw_masking::decide_raw_masking;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{Tile, CELL_LEVEL};
use crate::gct::nested_resolutions::NestedResolutions;

/// Where one pass searches: an area, the coarsest level a candidate in
/// it may be, and the resolutions of the complex tiles it is nested in.
struct SearchArea {
    /// The tile searched in.
    area: Tile,
    /// The coarsest level a candidate in it may be.
    coarsest_level: u8,
    /// The resolutions of the complex tiles `area` is nested in.
    nested: NestedResolutions,
}

/// Creates complex tiles from the greedy tiler's output -- the complex
/// tiling pyramid with only its placement bits set -- and returns it
/// complete: the placements, with every committed complex tile's size
/// offset added.
pub fn complex_tiler(placements: Pyramid) -> Pyramid {
    let raw_masked = decide_raw_masking(&placements);
    let mut complex_tiling = Pyramid::complex_tiling(placements, &raw_masked);

    let mut searched = vec![SearchArea { area: Tile::whole_bitmap(), coarsest_level: 0, nested: NestedResolutions::none() }];
    while !searched.is_empty() {
        let mut chosen = Vec::new();
        let mut counted = CountedBits::default();
        for search in &searched {
            for tile in search.area.tiles_at_size_offset(search.coarsest_level - search.area.level) {
                best_at_or_under(&mut complex_tiling, tile, &search.nested, &mut counted, &mut chosen);
            }
        }
        searched = commit(chosen, &mut complex_tiling);
    }
    complex_tiling
}

/// The smallest tile a complex tile can be: 4x4, so its resolution is
/// at least 2x2.
const FINEST_CANDIDATE_LEVEL: u8 = CELL_LEVEL - 2;

/// Adds to `chosen` the candidates at or under `tile` that save the most
/// bits between them, none overlapping, and returns what they save:
/// `tile`'s own candidate, or the best under each of its children,
/// whichever saves more -- `tile`'s own on a tie.
fn best_at_or_under(
    complex_tiling: &mut Pyramid,
    tile: Tile,
    nested: &NestedResolutions,
    counted: &mut CountedBits,
    chosen: &mut Vec<Candidate>,
) -> u64 {
    if tile.level > FINEST_CANDIDATE_LEVEL || nested.unmasking(complex_tiling.fields(tile), tile).is_some() {
        return 0;
    }
    // A placed tile says everything under it itself, but for the
    // children it masks: only those can hold candidates.
    let placed = complex_tiling.placed_at(tile);
    let own = match placed {
        Some(_) => None,
        None => Candidate::best_for(complex_tiling, tile, nested, counted),
    };
    let mut under = Vec::new();
    let under_saving: u64 = tile
        .children()
        .into_iter()
        .filter(|&child| placed.is_none_or(|placement| placement.masks(child)))
        .map(|child| best_at_or_under(complex_tiling, child, nested, counted, &mut under))
        .sum();
    match own {
        Some(candidate) if candidate.saving >= under_saving => {
            let saving = candidate.saving;
            chosen.push(candidate);
            saving
        }
        _ => {
            chosen.extend(under);
            under_saving
        }
    }
}

/// Commits `chosen`; returns where the next pass searches -- inside each
/// one committed.
fn commit(chosen: Vec<Candidate>, complex_tiling: &mut Pyramid) -> Vec<SearchArea> {
    let mut next = Vec::new();
    for candidate in chosen {
        complex_tiling.make_complex_tile(candidate.tile, candidate.size_offset);
        next.push(SearchArea {
            area: candidate.tile,
            coarsest_level: candidate.tile.level + 1,
            nested: candidate.nested.with_nested(candidate.tile.level + candidate.size_offset),
        });
    }
    next
}
