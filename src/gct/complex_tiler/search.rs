//! The complex tiler: groups the greedy tiler's placed tiles into complex
//! tiles, in one pass, bottom-up.
//!
//! Every tile the tree could reach is counted once, after its children:
//! its bits as the tiling stands, and its bits under one complex tile of
//! each resolution from its own level down to 1x1 -- what a candidate
//! above it would make of it. Each tile nothing is placed at is then
//! scored as a candidate at every resolution the grammar lets it take,
//! its children's bits read off their counts: the bits it saves are its
//! bits as it stands less its bits as that complex tile. Tiles that do
//! not overlap cost bits independently, so the candidates that save the
//! most between them are found on the way back up: a tile keeps its own
//! best candidate when that saves at least as much as the best its
//! children keep between them.
//!
//! Every count follows the grammar's widths ([`crate::gct::grammar`]),
//! as [`bit_cost`](super::bit_cost) spells them out for any tiling;
//! debug builds hold the counts at 16x16 and finer to it. Here there is
//! one case only: no complex tile yet, and at most the candidate above.
//! Complex tiles are never nested in one another, and the grammar has
//! no way to say it: a bind in a complex tile's body is always a tile,
//! and says no size offset (`docs/gct.md`).

use super::bit_cost::{bits_with, cell_list_header_bits, node_bits, payload_bits, Counting};
use super::raw_masking::decide_raw_masking;
use crate::gct::fixed_list::FixedList;
use crate::gct::grammar::*;
use crate::gct::grammar::bit_stream::MOST_BITS;
use crate::gct::grammar::cell_list;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::residual_prices::ResidualPrices;
use crate::gct::pyramids::complex_tiling::{ComplexTiling, Fields};
use crate::gct::pyramids::placements::Placement;
use crate::gct::tile::{tiles_in_level, Tile, CELL_LEVEL, CHILDREN, FINEST_PLACED_LEVEL, FLOOR_LEVEL};
use crate::Bitmap;

/// The smallest tile a complex tile can be: 4x4, the floor, so its
/// resolution is at least 2x2.
pub const FINEST_CANDIDATE_LEVEL: u8 = FLOOR_LEVEL;

/// The most candidates the search can commit: they never overlap, and
/// none is finer than 4x4, so no more than there are 4x4s.
const MOST_CANDIDATES: usize = tiles_in_level(FINEST_CANDIDATE_LEVEL);

/// A tile the search makes a complex tile, at its best size offset.
#[derive(Clone, Copy, Default)]
struct Candidate {
    /// The tile.
    tile: Tile,
    /// How many levels finer than the tile its resolution is.
    size_offset: u8,
    /// Whether, of 1x1 resolution, it says its cells as a cell list.
    cell_list: bool,
}

/// Room the complex tiler works in, allocated once at the most any
/// bitmap needs.
#[derive(Default)]
pub struct Scratch {
    /// The candidates the search keeps.
    chosen: FixedList<Candidate, MOST_CANDIDATES>,
}

/// Creates complex tiles from the greedy tiler's output -- the complex
/// tiling pyramid with its placements, the value bound above each tile
/// and the sizes bound under it -- and completes it in place: every
/// chosen complex tile's size offset added. Whether it chose any.
pub fn complex_tiler(complex_tiling: &mut ComplexTiling, bitmap: &Bitmap, residual_prices: &ResidualPrices, scratch: &mut Scratch) -> bool {
    decide_raw_masking(complex_tiling, residual_prices);
    scratch.chosen.clear();
    let counting = Counting { complex_tiling, bitmap, residual_prices };
    Search { counting, chosen: &mut scratch.chosen }.count(Tile::whole_bitmap());
    for candidate in scratch.chosen.iter() {
        if candidate.cell_list {
            complex_tiling.make_cell_list(candidate.tile);
        } else {
            complex_tiling.make_complex_tile(candidate.tile, candidate.size_offset);
        }
    }
    !scratch.chosen.is_empty()
}

/// Bits counts are held in: a stream's bits fit.
type Bits = u32;
const _: () = assert!(MOST_BITS <= Bits::MAX as usize);

/// A slot for every resolution a candidate above a tile can have, by the
/// resolution's level.
const RESOLUTION_SLOTS: usize = CELL_LEVEL as usize + 1;

/// What the search knows of a tile once it is counted.
#[derive(Clone, Copy)]
struct Counted {
    /// Its bits as the tiling stands, with no complex tile above it.
    without: Bits,
    /// Its bits as the tiling stands, masked in a complex tile's body:
    /// the same, less every bind's size offset, which a bind there does
    /// not say.
    without_in_body: Bits,
    /// Its bits under one complex tile of each resolution, by the
    /// resolution's level: meaningful from the tile's own level to 1x1.
    under: [Bits; RESOLUTION_SLOTS],
    /// What the best candidates at or under it save between them, none
    /// overlapping.
    best_saving: Bits,
}

/// One search over one bitmap's complex tiling.
struct Search<'a> {
    /// The tiling searched: the greedy tiler's placements, filled in.
    counting: Counting<'a>,
    /// The candidates kept so far.
    chosen: &'a mut FixedList<Candidate, MOST_CANDIDATES>,
}

/// The children of a tile the search goes into, and what each is to it.
struct Children {
    /// Which children are counted into the tile's bits, bit `i` for
    /// child `i`: those that are nodes of their own.
    counted: u8,
    /// Which are left to the binding above: no node, no bits.
    left: u8,
    /// Which the search goes into for candidates: every counted and
    /// left child.
    visited: u8,
}

impl Search<'_> {
    /// Counts `tile`, and everything under it the tree could reach, and
    /// keeps the best candidates at or under it.
    fn count(&mut self, tile: Tile) -> Counted {
        let here = self.counting.complex_tiling.fields(tile);
        debug_assert!(here.complex_tile_size_offset().is_none(), "{tile:?}: a complex tile before the search");
        let children = self.children_of(tile, here);
        let child_tiles = tile.children();
        let mut child_counts: [Option<Counted>; CHILDREN as usize] = [None; CHILDREN as usize];
        let mut best_under_children = 0;
        let chosen_before_children = self.chosen.len();
        for (index, &child) in child_tiles.iter().enumerate() {
            if children.visited & 1 << index != 0 {
                let counted = self.count(child);
                best_under_children += counted.best_saving;
                child_counts[index] = Some(counted);
            }
        }
        // The children's counts summed once: those counted into the tile's
        // bits, and every visited one, as a candidate's children are.
        let (mut counted_without, mut counted_without_in_body) = (0, 0);
        let (mut counted_under, mut visited_under) = ([0; RESOLUTION_SLOTS], [0; RESOLUTION_SLOTS]);
        for (index, counted) in child_counts.iter().enumerate() {
            let Some(counted) = counted else { continue };
            let is_counted = children.counted & 1 << index != 0;
            if is_counted {
                counted_without += counted.without;
                counted_without_in_body += counted.without_in_body;
            }
            for resolution in tile.level + 1..=CELL_LEVEL {
                let bits = counted.under[resolution as usize];
                visited_under[resolution as usize] += bits;
                if is_counted {
                    counted_under[resolution as usize] += bits;
                }
            }
        }

        let own = own_bits(tile, here, children.left, self.counting.residual_prices);
        let own_in_body = own - bind_size_offset_bits(tile, here);
        let without = own + counted_without;
        let without_in_body = own_in_body + counted_without_in_body;
        debug_assert_matches_reference(self.counting, tile, here, without);
        let mut under = [0; RESOLUTION_SLOTS];
        under[tile.level as usize] = if here.entirely_bound_at(tile.level) { unmasked_bits(0) } else { without_in_body + MASK_BIT_WIDTH as Bits };
        let left = children.left.count_ones() as Bits;
        for resolution in tile.level + 1..=CELL_LEVEL {
            under[resolution as usize] = if here.entirely_bound_at(resolution) {
                unmasked_bits(resolution - tile.level)
            } else {
                // Masked in the candidate: its mask bit, its own bits and
                // each counted child's under it -- and, just above the
                // candidate's resolution, every child left to the binding
                // above is bound whole at it, so unmasked instead: a mask
                // bit and a payload bit each, and no flip bit or child
                // mask.
                let mut bits = own_in_body + MASK_BIT_WIDTH as Bits + counted_under[resolution as usize];
                if resolution == tile.level + 1 && left > 0 {
                    bits = bits + left * unmasked_bits(0) - LEAVING_BITS;
                }
                bits
            };
        }

        let own = (here.placed().is_none() && tile.level <= FINEST_CANDIDATE_LEVEL)
            .then(|| self.best_candidate(tile, here, &visited_under, without))
            .flatten();
        let best_saving = match own {
            Some((candidate, saving)) if saving >= best_under_children => {
                self.chosen.truncate(chosen_before_children);
                self.chosen.push(candidate);
                saving
            }
            _ => best_under_children,
        };
        Counted { without, without_in_body, under, best_saving }
    }

    /// Which of `tile`'s children the search goes into and counts: none
    /// under the floor or a tile placed whole, the children a masking
    /// tile masks, and every child of a divide, but those left to the
    /// binding above, which are only visited.
    fn children_of(&self, tile: Tile, here: Fields) -> Children {
        if tile.level == FLOOR_LEVEL {
            return Children { counted: 0, left: 0, visited: 0 };
        }
        if let Some(placement) = here.placed() {
            let masked = placement.masked_children();
            return Children { counted: masked, left: 0, visited: masked };
        }
        let mut left = 0;
        for (index, (child, fields)) in tile.children().into_iter().zip(self.counting.complex_tiling.children_fields(tile)).enumerate() {
            if fields.left_to_binding_above(child, here.bound_above(), &NestedResolutions::none()) {
                left |= 1 << index;
            }
        }
        Children { counted: ALL_CHILDREN & !left, left, visited: ALL_CHILDREN }
    }

    /// `tile`'s best candidate, and the bits it saves, if any saves
    /// bits: tried at every resolution the grammar lets it take, the
    /// coarsest first, and at 1x1 as a cell list too -- the first that
    /// saves the most kept. `children_under` is its children's bits
    /// summed under each resolution, and `without` its bits as it
    /// stands.
    fn best_candidate(&self, tile: Tile, here: Fields, children_under: &[Bits; RESOLUTION_SLOTS], without: Bits) -> Option<(Candidate, Bits)> {
        let mut best: Option<(Candidate, Bits)> = None;
        for resolution in tried_resolutions(here, tile.level) {
            let size_offset = resolution - tile.level;
            let mut bits = complex_tile_header_bits(tile.level, size_offset);
            if here.entirely_bound_at(resolution) {
                if resolution == CELL_LEVEL {
                    bits += PAYLOAD_MODE_WIDTH as Bits;
                }
                bits += payload_bits(size_offset) as Bits;
            } else {
                bits += children_under[resolution as usize];
            }
            debug_assert_candidate_matches_reference(self.counting, tile, here.as_complex_tile(size_offset), bits);
            let mut cell_list = false;
            if resolution == CELL_LEVEL {
                // The cells as a cell list, when strictly cheaper --
                // counted only when the fewest bits it could take are.
                let header = cell_list_header_bits(tile.level) as Bits;
                if header + (cell_list::least_bits(self.counting.bitmap, tile) as Bits) < bits {
                    let listed = header + cell_list::bits(self.counting.bitmap, tile) as Bits;
                    if listed < bits {
                        (bits, cell_list) = (listed, true);
                    }
                }
            }
            let Some(saving) = without.checked_sub(bits).filter(|&saving| saving > 0) else { continue };
            if best.is_none_or(|(_, best_saving)| saving > best_saving) {
                best = Some((Candidate { tile, size_offset, cell_list }, saving));
            }
        }
        best
    }
}

/// The bits a tile unmasked in the candidate above it takes: its mask
/// bit, and its values, one for each tile of the candidate's resolution
/// `levels_finer` levels under it.
fn unmasked_bits(levels_finer: u8) -> Bits {
    MASK_BIT_WIDTH as Bits + payload_bits(levels_finer) as Bits
}

/// What a divide that leaves children to the binding above spends on
/// that and a divide that leaves none does not: its flip bit and child
/// mask.
const LEAVING_BITS: Bits = (FLIP_WIDTH + CHILD_MASK_WIDTH) as Bits;

/// A complex tile's bits at a tile of `level`, of `size_offset`, before
/// its payload or children: its leaf and code bits, its size offset and,
/// where it may mask, its mask-present bit.
fn complex_tile_header_bits(level: u8, size_offset: u8) -> Bits {
    let mut bits = (LEAF_WIDTH + CODE_WIDTH + resolution_width(level)) as Bits;
    if complex_tile_may_mask(size_offset) {
        bits += MASK_PRESENT_WIDTH as Bits;
    }
    bits
}

/// The bits `tile`, whose fields are `here`, takes itself as the tiling
/// stands, its counted children's bits aside; `left` the children a
/// divide leaves to the binding above; a residual block at its price in
/// `residual_prices`.
fn own_bits(tile: Tile, here: Fields, left: u8, residual_prices: &ResidualPrices) -> Bits {
    let leaf_and_code = (LEAF_WIDTH + CODE_WIDTH) as Bits;
    let mask_present = if copy_or_divide_may_mask(tile.level) { MASK_PRESENT_WIDTH as Bits } else { 0 };
    match here.placed() {
        Some(Placement::Bound { masked_children: 0, .. }) => leaf_and_code + resolution_width(tile.level) as Bits + payload_bits(0) as Bits,
        // Spelled as a divide that masks and flips the value bound above.
        Some(Placement::Bound { .. }) => (MASKING_DIVIDE_HEADER_WIDTH + CHILD_MASK_WIDTH) as Bits,
        Some(copy @ Placement::Copied { .. }) => {
            let child_mask = if copy.masks_any() { CHILD_MASK_WIDTH as Bits } else { 0 };
            leaf_and_code + (FAR_WIDTH + DIRECTION_WIDTH) as Bits + mask_present + child_mask
        }
        // A residual block, and its cells in the last pass, at its price.
        None if tile.level == FLOOR_LEVEL => LEAF_WIDTH as Bits + residual_prices.of(tile) as Bits,
        None => LEAF_WIDTH as Bits + mask_present + if left != 0 { LEAVING_BITS } else { 0 },
    }
}

/// What `tile`, whose fields are `here`, spends of its own bits on a
/// bind's size offset: none but for a bind, which in a complex tile's
/// body does not say it.
fn bind_size_offset_bits(tile: Tile, here: Fields) -> Bits {
    match here.placed() {
        Some(Placement::Bound { masked_children: 0, .. }) => resolution_width(tile.level) as Bits,
        _ => 0,
    }
}

/// The resolutions a candidate at a tile of `level`, whose fields are
/// `here`, is tried at, coarsest first: from one level finer to 2x2,
/// and 1x1 where the grammar can name it -- but not one coarser than
/// 1x1 no whole bind under it is placed at (nothing to unmask), nor one
/// level finer unless it is entirely bound there (the grammar gives size
/// offset `1` no way to mask).
fn tried_resolutions(here: Fields, level: u8) -> impl Iterator<Item = u8> {
    let finest = if raw_resolution_fits(level) { CELL_LEVEL } else { FINEST_PLACED_LEVEL };
    (level + 1..=finest).filter(move |&resolution| {
        (resolution == CELL_LEVEL || here.any_bound_under(resolution)) && (resolution > level + 1 || here.entirely_bound_at(resolution))
    })
}

/// The finest tiles debug builds check against the reference count,
/// 16x16 and finer: coarser ones would count too much of the bitmap
/// again to check every one.
const FINEST_CHECKED_LEVEL: u8 = CELL_LEVEL - 4;

/// In debug builds, for a tile of 16x16 or finer, that `without` is what
/// the reference count ([`bits_with`]) gives it.
fn debug_assert_matches_reference(counting: Counting, tile: Tile, here: Fields, without: Bits) {
    if cfg!(debug_assertions) && tile.level >= FINEST_CHECKED_LEVEL {
        let reference = bits_with(counting, tile, here, &mut NestedResolutions::none(), here.bound_above());
        assert_eq!(without as u64, reference, "{tile:?}: the search's count is not the reference count");
    }
}

/// In debug builds, for a candidate of 16x16 or finer, that `bits` is
/// what the reference count gives the tile with `fields` as its fields.
fn debug_assert_candidate_matches_reference(counting: Counting, tile: Tile, fields: Fields, bits: Bits) {
    if cfg!(debug_assertions) && tile.level >= FINEST_CHECKED_LEVEL {
        let reference = node_bits(counting, tile, fields, &mut NestedResolutions::none(), fields.bound_above(), &mut |child, fields, inside, bound_above| {
            bits_with(counting, child, fields, inside, bound_above)
        });
        assert_eq!(bits as u64, reference, "{tile:?}: the search's candidate count is not the reference count");
    }
}

/// Every child's bit set.
const ALL_CHILDREN: u8 = (1 << CHILDREN) - 1;
