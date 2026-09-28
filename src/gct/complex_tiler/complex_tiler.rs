//! The complex tiler, the second pass: groups the greedy tiler's placed
//! tiles into complex tiles, nested as deep as they keep paying. Its
//! output is one pyramid, the
//! [`complex_tiling`](crate::gct::pyramids::complex_tiling): the
//! placements, each tile's single bound size, and which tiles are
//! complex tiles at what size offset -- all the tree is read from.
//! Every decision is made from the tiles the greedy tiler placed, but
//! for one: what a point list costs, read off the tile's cells.
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

use super::cost_lanes::CostLanes;
use super::complex_tile_candidates::Candidate;
use super::raw_masking::{decide_raw_masking, MOST_RAW_MASKED};
use crate::fixed_list::FixedList;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::pyramids::placements::{binding_above, Placement};
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{tiles_down_to, Tile, CELL_LEVEL, CHILDREN_ACROSS};
use crate::gct::nested_resolutions::NestedResolutions;
use crate::Bitmap;

/// Where one pass searches: an area, the coarsest level a candidate in
/// it may be, and the resolutions of the complex tiles it is nested in.
#[derive(Clone, Copy, Default)]
struct SearchArea {
    /// The tile searched in.
    area: Tile,
    /// The coarsest level a candidate in it may be.
    coarsest_level: u8,
    /// The resolutions of the complex tiles `area` is nested in.
    nested: NestedResolutions,
}

/// The most candidates a pass can commit, or search inside: one a tile,
/// from the whole bitmap down to 4x4.
const MOST_CANDIDATES: usize = tiles_down_to(FINEST_CANDIDATE_LEVEL);

/// The most roots a search area has: a complex tile's children.
const MOST_ROOTS: usize = (CHILDREN_ACROSS * CHILDREN_ACROSS) as usize;

/// Room the complex tiler works in, allocated once at the most any
/// bitmap needs.
#[derive(Default)]
pub struct Scratch {
    /// The tiles a complex tile of 1x1 resolution masks.
    raw_masked: FixedList<Tile, MOST_RAW_MASKED>,
    /// Every count a search area's candidates ask for.
    lanes: CostLanes,
    /// A search area's roots, each with the value bound above it: the
    /// whole bitmap, or the four children of a complex tile.
    roots: FixedList<(Tile, bool), MOST_ROOTS>,
    /// Where this pass searches.
    searched: FixedList<SearchArea, MOST_CANDIDATES>,
    /// Where the next pass searches.
    next: FixedList<SearchArea, MOST_CANDIDATES>,
    /// The candidates this pass commits.
    chosen: FixedList<Candidate, MOST_CANDIDATES>,
}

/// Creates complex tiles from the greedy tiler's output -- the complex
/// tiling pyramid with only its placement bits set -- and completes it
/// in place: the placements, with every committed complex tile's size
/// offset added.
pub fn complex_tiler(complex_tiling: &mut Pyramid, bitmap: &Bitmap, scratch: &mut Scratch) {
    let Scratch { raw_masked, lanes, roots, searched, next, chosen } = scratch;
    decide_raw_masking(complex_tiling, raw_masked);
    complex_tiling.fill_in(raw_masked);

    searched.clear();
    searched.push(SearchArea { area: Tile::whole_bitmap(), coarsest_level: 0, nested: NestedResolutions::none() });
    while !searched.is_empty() {
        chosen.clear();
        for search in searched.iter() {
            roots.clear();
            roots.extend(
                search
                    .area
                    .tiles_at_size_offset(search.coarsest_level - search.area.level)
                    .map(|tile| (tile, binding_above(tile, |at| complex_tiling.placed_at(at)))),
            );
            lanes.fill(complex_tiling, bitmap, roots.iter().copied(), &search.nested);
            for &(tile, bound_above) in roots.iter() {
                best_at_or_under(complex_tiling, bitmap, lanes, tile, &search.nested, bound_above, chosen);
            }
        }
        next.clear();
        commit(chosen, complex_tiling, next);
        std::mem::swap(searched, next);
    }
}

/// The smallest tile a complex tile can be: 4x4, so its resolution is
/// at least 2x2.
const FINEST_CANDIDATE_LEVEL: u8 = CELL_LEVEL - 2;

/// Adds to `chosen` the candidates at or under `tile` that save the most
/// bits between them, none overlapping, and returns what they save:
/// `tile`'s own candidate, or the best under each of its children,
/// whichever saves more -- `tile`'s own on a tie. `bound_above` is the
/// value bound above `tile`; every count is read from `lanes`.
fn best_at_or_under(
    complex_tiling: &Pyramid,
    bitmap: &Bitmap,
    lanes: &CostLanes,
    tile: Tile,
    nested: &NestedResolutions,
    bound_above: bool,
    chosen: &mut FixedList<Candidate, MOST_CANDIDATES>,
) -> u64 {
    if tile.level > FINEST_CANDIDATE_LEVEL || nested.unmasking(complex_tiling.fields(tile), tile).is_some() {
        return 0;
    }
    // A placed tile says everything under it itself, but for the
    // children it masks: only those can hold candidates.
    let placed = complex_tiling.placed_at(tile);
    let own = match placed {
        Some(_) => None,
        None => Candidate::best_for(complex_tiling, bitmap, lanes, tile, nested, bound_above),
    };
    let bound_inside = match placed {
        Some(Placement::Bound { value, .. }) => value,
        _ => bound_above,
    };
    // The best under the children go on `chosen` first, to be taken off
    // again if the tile's own does better.
    let under_from = chosen.len();
    let under_saving: u64 = tile
        .children()
        .into_iter()
        .filter(|&child| placed.is_none_or(|placement| placement.masks(child)))
        .map(|child| best_at_or_under(complex_tiling, bitmap, lanes, child, nested, bound_inside, chosen))
        .sum();
    match own {
        Some(candidate) if candidate.saving >= under_saving => {
            chosen.truncate(under_from);
            chosen.push(candidate);
            candidate.saving
        }
        _ => under_saving,
    }
}

/// Commits `chosen`, adding to `next` where the next pass searches --
/// inside each one committed, but a point list: it says every cell
/// under it itself.
fn commit(chosen: &[Candidate], complex_tiling: &mut Pyramid, next: &mut FixedList<SearchArea, MOST_CANDIDATES>) {
    for candidate in chosen {
        if candidate.point_list {
            complex_tiling.make_point_list(candidate.tile);
            continue;
        }
        complex_tiling.make_complex_tile(candidate.tile, candidate.size_offset);
        next.push(SearchArea {
            area: candidate.tile,
            coarsest_level: candidate.tile.level + 1,
            nested: candidate.nested.with_nested(candidate.tile.level + candidate.size_offset),
        });
    }
}
