//! The cost pyramid: every bit count the complex tiler asks for in one
//! search area, counted before any is asked, and held -- memory for the
//! counting it saves.
//!
//! For every tile, its bits nested as the area is, with no candidate
//! above it, and for each resolution `r` how much one more complex tile,
//! of resolution `r`, above it -- the candidate being scored -- takes off
//! them: its change ([`crate::gct::pyramids::costs`]). The tile's bits
//! under the candidate are the first less the change.
//!
//! Every change follows from the tile's own fields and its children's
//! changes, for all resolutions at once, a few steps each -- no count is
//! made again for each candidate. Under a candidate of resolution `r`, a
//! tile finer than `r` is unchanged, `r` unmasking nothing in it and
//! adding no mask bit. Any other tile takes the candidate's mask bit
//! first, the candidate being the complex tile nearest to it: if it is
//! entirely bound at `r`, that unmasks it -- its bits are that mask bit
//! and its payload; if not, its bits are its own with that mask bit
//! more, and each child it counts changed as that child is. But for one
//! thing more: a divide one level coarser than `r` leaves no child to
//! the binding above under the candidate -- every such child is bound
//! whole at `r`, so unmasked -- and spends no flip bit and child mask on
//! them, but a mask bit and a payload bit each. With no complex tile
//! under a search area's roots while its counts are filled, nothing
//! else changes.
//!
//! Filled in two walks: down from the area's search roots, collecting
//! the tiles a count can reach -- all a tile placed nothing says, the
//! children a masking tile masks, nothing under a tile placed whole or
//! entirely unmasked in the area; then back up, a level at a time, each
//! tile's bits and changes from its children's, by the rules the bit
//! count ([`super::bit_cost`]) spells out. The value bound above a tile
//! is its own field in the complex tiling, not carried down. A 2x2's
//! bits depend only on whether it is one tile, whether a raw complex
//! tile masks it, and whether the candidate reaches inside it -- at 2x2
//! or 1x1 -- so an area's twelve 2x2 counts are made once and read off.
//! Nothing is changed while the pyramid is read: it is a snapshot of the
//! tiling as the pass found it.

use super::bit_cost::{node_bits, payload_bits};
use crate::fixed_list::FixedList;
use crate::gct::grammar::{CHILD_MASK_WIDTH, FLIP_WIDTH, LEAF_WIDTH, MASK_BIT_WIDTH};
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::complex_tiling::{ComplexTiling, Fields};
use crate::gct::pyramids::costs::{Changes, Costs, FINEST_HELD, NO_CANDIDATE, RESOLUTIONS};
use crate::gct::pyramids::placements::Placement;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{cells_in_tile, tiles_down_to, Tile, ALL_CHILDREN, CELL_LEVEL, FLOOR_LEVEL};
use crate::Bitmap;

/// The most tiles a count can reach: every tile held, whole bitmap to
/// the finest level.
const MOST_REACHED: usize = tiles_down_to(FINEST_HELD);

/// The mask bits a divide spends on a child mask, and the flip bit:
/// what one that leaves children to the binding above spends, and one
/// that leaves none does not.
const LEAVING_BITS: i32 = (FLIP_WIDTH + CHILD_MASK_WIDTH) as i32;

/// Set in a 2x2's kind when it is one tile, not four raw cells.
const WHOLE_KIND: usize = 1;
/// Set in a 2x2's kind when a complex tile of 1x1 resolution masks it.
const RAW_MASKED_KIND: usize = 2;
/// 2x2 kinds.
const FLOOR_KINDS: usize = WHOLE_KIND + RAW_MASKED_KIND + 1;

/// What a 2x2 is, as far as its bits go: one tile or four raw cells,
/// and whether a complex tile of 1x1 resolution masks it -- its value,
/// and anything else, change nothing.
fn floor_kind(here: Fields) -> usize {
    let whole = if here.placed().is_some_and(Placement::is_whole_bind) { WHOLE_KIND } else { 0 };
    whole | if here.raw_masks() { RAW_MASKED_KIND } else { 0 }
}

/// Candidates a 2x2's bits can differ under: none, and the two
/// resolutions that reach inside a 2x2 -- itself, and 1x1.
const FLOOR_CANDIDATES: [u8; 3] = [NO_CANDIDATE, FLOOR_LEVEL, CELL_LEVEL];
/// Where no candidate is in [`FLOOR_CANDIDATES`]...
const UNDER_NO_CANDIDATE: usize = 0;
/// ...one of 2x2 resolution...
const UNDER_FLOOR_CANDIDATE: usize = 1;
/// ...and one of 1x1 resolution.
const UNDER_CELL_CANDIDATE: usize = 2;
const _: () = assert!(
    FLOOR_CANDIDATES[UNDER_NO_CANDIDATE] == NO_CANDIDATE
        && FLOOR_CANDIDATES[UNDER_FLOOR_CANDIDATE] == FLOOR_LEVEL
        && FLOOR_CANDIDATES[UNDER_CELL_CANDIDATE] == CELL_LEVEL
);

/// Which of [`FLOOR_CANDIDATES`] a candidate of `resolution` counts as,
/// for a 2x2: none, if it is coarser.
fn floor_candidate(resolution: u8) -> usize {
    match resolution {
        FLOOR_LEVEL => UNDER_FLOOR_CANDIDATE,
        CELL_LEVEL => UNDER_CELL_CANDIDATE,
        _ => UNDER_NO_CANDIDATE,
    }
}

/// A 2x2's bits, by its kind and the candidate above it, in one search
/// area: 1 a leaf bit and a value bit, or a residual bit and four raw
/// cells -- after its mask bits, nearest complex tile first, unless one
/// unmasks it, when its bits are those mask bits and that one's payload.
/// Nothing else of a 2x2 or its nesting counts, so the area's twelve
/// counts are made once, and every 2x2's is read off.
fn floor_bits(base: &NestedResolutions) -> [[u64; FLOOR_CANDIDATES.len()]; FLOOR_KINDS] {
    let floor = Tile { level: FLOOR_LEVEL, x: 0, y: 0 };
    std::array::from_fn(|kind| {
        let (whole, raw_masked) = (kind & WHOLE_KIND != 0, kind & RAW_MASKED_KIND != 0);
        FLOOR_CANDIDATES.map(|candidate| {
            let nested = if candidate == NO_CANDIDATE { *base } else { base.with_nested(candidate) };
            let mut mask_bits = 0;
            for nesting in nested.able_to_unmask(floor) {
                mask_bits += MASK_BIT_WIDTH as u64;
                let resolution = nested.resolution(nesting);
                let unmasked = if resolution == CELL_LEVEL { !raw_masked } else { whole && resolution == FLOOR_LEVEL };
                if unmasked {
                    return mask_bits + payload_bits(resolution - FLOOR_LEVEL);
                }
            }
            mask_bits + LEAF_WIDTH as u64 + if whole { payload_bits(0) } else { cells_in_tile(FLOOR_LEVEL) }
        })
    })
}

/// The cost pyramid, and room to collect the tiles to fill it for, all
/// allocated once.
pub struct CostPyramid {
    /// Each tile's bits with no candidate, and its changes: a [costs
    /// pyramid](crate::gct::pyramids::costs).
    counts: Pyramid,
    /// The tiles a count can reach, each after its parent.
    reached: FixedList<Tile, MOST_REACHED>,
    /// Every 2x2's bits in the area, by its kind and candidate.
    floor: [[u64; FLOOR_CANDIDATES.len()]; FLOOR_KINDS],
}

impl Default for CostPyramid {
    /// The pyramid allocated, nothing counted.
    fn default() -> Self {
        Self {
            counts: Pyramid::costs(),
            reached: FixedList::new(),
            floor: [[0; FLOOR_CANDIDATES.len()]; FLOOR_KINDS],
        }
    }
}

/// A tile's changes, the tile at `tile` with fields `here` and bits
/// `without` with no candidate: `under` its counted children's changes
/// summed, `left` how many children it leaves to the binding above.
fn changes_of(tile: Tile, here: Fields, without: u64, under: &Changes, left: i32) -> Changes {
    let mask = MASK_BIT_WIDTH as i32;
    let mut changes = [0; RESOLUTIONS];
    for (change_index, change) in changes.iter_mut().enumerate() {
        let resolution = change_index as u8 + 1;
        if tile.level > resolution {
            continue;
        }
        *change = if here.entirely_bound_at(resolution) {
            // Unmasked by the candidate: its mask bit and payload.
            without as i32 - mask - payload_bits(resolution - tile.level) as i32
        } else if resolution == tile.level + 1 && left > 0 {
            // A divide leaving children to the binding above, each bound
            // whole at the candidate's resolution: unmasked, a mask bit and
            // a payload bit each, and no flip bit or child mask.
            -mask + under[change_index] + LEAVING_BITS - left * (mask + payload_bits(0) as i32)
        } else {
            -mask + under[change_index]
        };
    }
    changes
}

impl CostPyramid {
    /// Counts, for every tile a count can reach from `roots`, all of one
    /// level, its bits and its changes under every candidate resolution,
    /// in a search area nested in `base`.
    pub fn fill(&mut self, complex_tiling: &Pyramid, bitmap: &Bitmap, roots: &[Tile], base: &NestedResolutions) {
        debug_assert!(roots.iter().all(|root| root.level == roots[0].level), "roots of one level");
        self.floor = floor_bits(base);
        self.reached.clear();
        self.reached.extend(roots.iter().copied().filter(|root| root.level <= FINEST_HELD));
        // Down, breadth first, so a level at a time: every tile reached is
        // added after its parent, and nothing is under the finest level.
        let mut next_to_visit = 0;
        while next_to_visit < self.reached.len() {
            let tile = self.reached[next_to_visit];
            next_to_visit += 1;
            if tile.level == FINEST_HELD {
                break;
            }
            let here = complex_tiling.fields(tile);
            let reached_children = match here.placed() {
                None if base.unmasking(here, tile).is_none() => ALL_CHILDREN,
                None => 0,
                Some(Placement::Bound { masked_children, .. } | Placement::Copied { masked_children, .. }) => masked_children,
            };
            for child in tile.children() {
                if reached_children & 1 << child.child_index() != 0 {
                    self.reached.push(child);
                }
            }
        }
        // Back up, in reverse: every tile counted after its children.
        for reached_index in (0..self.reached.len()).rev() {
            let tile = self.reached[reached_index];
            let here = complex_tiling.fields(tile);
            debug_assert!(here.complex_tile_size_offset().is_none(), "{tile:?}: a complex tile under a search area's roots");
            let mut under = [0; RESOLUTIONS];
            let without = node_bits(complex_tiling, bitmap, tile, here, &mut base.clone(), here.bound_above(), &mut |child, fields, _, _| {
                let (bits, changes) = self.without_and_changes(child, fields);
                for (sum, change) in under.iter_mut().zip(changes) {
                    *sum += change;
                }
                bits
            });
            // How many children a divide leaves to the binding above.
            let mut left = 0;
            if here.placed().is_none() && base.unmasking(here, tile).is_none() {
                let children = complex_tiling.children_fields(tile);
                for (child, fields) in tile.children().into_iter().zip(children) {
                    left += fields.left_to_binding_above(child, here.bound_above(), base) as i32;
                }
            }
            let changes = changes_of(tile, here, without, &under, left);
            self.counts.set_counts(tile, without, &changes);
        }
    }

    /// `tile`'s bits with no candidate, nested as the area is, and its
    /// changes: held, or, for a 2x2, whose fields are `here`, read off the
    /// area's 2x2 counts.
    fn without_and_changes(&self, tile: Tile, here: Fields) -> (u64, Changes) {
        if tile.level > FINEST_HELD {
            let bits = &self.floor[floor_kind(here)];
            let without = bits[UNDER_NO_CANDIDATE];
            let mut changes = [0; RESOLUTIONS];
            changes[FLOOR_LEVEL as usize - 1] = without as i32 - bits[UNDER_FLOOR_CANDIDATE] as i32;
            changes[CELL_LEVEL as usize - 1] = without as i32 - bits[UNDER_CELL_CANDIDATE] as i32;
            return (without, changes);
        }
        (self.counts.without(tile), self.counts.changes(tile))
    }

    /// `tile`'s bits under a candidate of `resolution` in the area: held,
    /// or, for a 2x2, whose fields are `here`, read off the area's 2x2
    /// counts.
    pub fn child_bits(&self, tile: Tile, here: Fields, resolution: u8) -> u64 {
        if tile.level > FINEST_HELD {
            return self.floor[floor_kind(here)][floor_candidate(resolution)];
        }
        let without = self.counts.without(tile);
        if resolution == NO_CANDIDATE {
            return without;
        }
        (without as i64 - self.counts.change(tile, resolution) as i64) as u64
    }

    /// `tile`'s bits with no candidate above it, nested in the area's
    /// own nesting.
    pub fn without(&self, tile: Tile) -> u64 {
        self.counts.without(tile)
    }
}
