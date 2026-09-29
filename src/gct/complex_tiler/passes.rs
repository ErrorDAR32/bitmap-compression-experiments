//! The complex tiler, the second pass: groups the greedy tiler's placed
//! tiles into complex tiles, nested as deep as they keep paying. Its
//! output is one pyramid, the [`complex_tiling`](crate::gct::pyramids::complex_tiling): the
//! placements, each tile's single bound size, and which tiles are
//! complex tiles at what size offset -- all the tree is read from.
//! Every decision is made from the tiles the greedy tiler placed, but
//! for one: what a cell list costs, read off the tile's cells.
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

use super::cost_pyramid::CostPyramid;
use super::complex_tile_candidates::{Candidate, FINEST_CANDIDATE_LEVEL};
use super::raw_masking::decide_raw_masking;
use crate::gct::fixed_list::FixedList;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::tile::{tiles_in_level, Tile};
use crate::gct::nested_resolutions::NestedResolutions;
use crate::Bitmap;

/// The most candidates a pass can commit: they never overlap, and none
/// is finer than 4x4, so no more than there are 4x4s.
const MOST_CANDIDATES: usize = tiles_in_level(FINEST_CANDIDATE_LEVEL);

/// Room the complex tiler works in, allocated once at the most any
/// bitmap needs.
#[derive(Default)]
pub struct Scratch {
    /// Every count a search area's candidates ask for.
    costs: CostPyramid,
    /// The candidates the pass before committed: this pass searches
    /// inside each.
    committed: FixedList<Candidate, MOST_CANDIDATES>,
    /// The candidates this pass commits.
    chosen: FixedList<Candidate, MOST_CANDIDATES>,
}

/// Creates complex tiles from the greedy tiler's output -- the complex
/// tiling pyramid with its placements [filled in](ComplexTiling::fill_in)
/// -- and completes it in place: every committed complex tile's size
/// offset added.
pub fn complex_tiler(complex_tiling: &mut ComplexTiling, bitmap: &Bitmap, scratch: &mut Scratch) {
    let Scratch { costs, committed, chosen } = scratch;
    decide_raw_masking(complex_tiling);

    chosen.clear();
    search(complex_tiling, bitmap, costs, &[Tile::whole_bitmap()], &NestedResolutions::none(), chosen);
    while !chosen.is_empty() {
        commit(chosen, complex_tiling);
        std::mem::swap(committed, chosen);
        chosen.clear();
        // Inside every complex tile committed, but a cell list: it says
        // every cell under it itself.
        for candidate in committed.iter().filter(|candidate| !candidate.cell_list) {
            let nested = candidate.nested.with_nested(candidate.tile.level + candidate.size_offset);
            search(complex_tiling, bitmap, costs, &candidate.tile.children(), &nested, chosen);
        }
    }
}

/// Adds to `chosen` the candidates at or under `roots`, all nested in
/// `nested`, that save the most bits between them, none overlapping --
/// every count read from `costs`, filled for them first.
fn search(
    complex_tiling: &ComplexTiling,
    bitmap: &Bitmap,
    costs: &mut CostPyramid,
    roots: &[Tile],
    nested: &NestedResolutions,
    chosen: &mut FixedList<Candidate, MOST_CANDIDATES>,
) {
    costs.fill(complex_tiling, bitmap, roots, nested);
    for &root in roots {
        best_at_or_under(complex_tiling, bitmap, costs, root, nested, chosen);
    }
}

/// Adds to `chosen` the candidates at or under `tile` that save the most
/// bits between them, none overlapping, and returns what they save:
/// `tile`'s own candidate, or the best under each of its children,
/// whichever saves more -- `tile`'s own on a tie. Every count is read
/// from `costs`.
fn best_at_or_under(
    complex_tiling: &ComplexTiling,
    bitmap: &Bitmap,
    costs: &CostPyramid,
    tile: Tile,
    nested: &NestedResolutions,
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
        None => Candidate::best_for(complex_tiling, bitmap, costs, tile, nested),
    };
    // The best under the children go on `chosen` first, to be taken off
    // again if the tile's own does better.
    let under_from = chosen.len();
    let under_saving: u64 = tile
        .children()
        .into_iter()
        .filter(|&child| placed.is_none_or(|placement| placement.masks(child)))
        .map(|child| best_at_or_under(complex_tiling, bitmap, costs, child, nested, chosen))
        .sum();
    match own {
        Some((candidate, saving)) if saving >= under_saving => {
            chosen.truncate(under_from);
            chosen.push(candidate);
            saving
        }
        _ => under_saving,
    }
}

/// Makes every candidate in `chosen` a complex tile, or a cell list.
fn commit(chosen: &[Candidate], complex_tiling: &mut ComplexTiling) {
    for candidate in chosen {
        if candidate.cell_list {
            complex_tiling.make_cell_list(candidate.tile);
        } else {
            complex_tiling.make_complex_tile(candidate.tile, candidate.size_offset);
        }
    }
}
