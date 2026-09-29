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
//! is its own field in the complex tiling, not carried down. The 4x4
//! floor, the finest level held, has no child node to count. Nothing is changed while the pyramid is read: it is a snapshot of the
//! tiling as the pass found it.

use super::bit_cost::{node_bits, payload_bits};
use crate::gct::fixed_list::FixedList;
use crate::gct::grammar::{CHILD_MASK_WIDTH, FLIP_WIDTH, MASK_BIT_WIDTH};
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::complex_tiling::{ComplexTiling, Fields};
use crate::gct::pyramids::costs::{Changes, Costs, FINEST_HELD, NO_CANDIDATE, RESOLUTIONS};
use crate::gct::pyramids::placements::Placement;
use crate::gct::tile::{tiles_down_to, Tile, ALL_CHILDREN};
use crate::Bitmap;

/// The most tiles a count can reach: every tile held, whole bitmap to
/// the finest level.
const MOST_REACHED: usize = tiles_down_to(FINEST_HELD);

/// The mask bits a divide spends on a child mask, and the flip bit:
/// what one that leaves children to the binding above spends, and one
/// that leaves none does not.
const LEAVING_BITS: i32 = (FLIP_WIDTH + CHILD_MASK_WIDTH) as i32;

/// The cost pyramid, and room to collect the tiles to fill it for, all
/// allocated once.
pub struct CostPyramid {
    /// Each tile's bits with no candidate, and its changes: a [costs
    /// pyramid](crate::gct::pyramids::costs).
    counts: Costs,
    /// The tiles a count can reach, each after its parent.
    reached: FixedList<Tile, MOST_REACHED>,
}

impl Default for CostPyramid {
    /// The pyramid allocated, nothing counted.
    fn default() -> Self {
        Self { counts: Costs::new(), reached: FixedList::new() }
    }
}

/// A tile's changes, the tile at `tile` with fields `here` and bits
/// `without` with no candidate: `under` its counted children's changes
/// summed, `left` how many children it leaves to the binding above.
fn changes_of(tile: Tile, here: Fields, without: u64, under: &Changes, left: i32) -> Changes {
    std::array::from_fn(|change_index| change(tile, here, without, change_index as u8 + 1, under[change_index], left))
}

/// A tile's change under a candidate of `resolution`, the tile at `tile`
/// with fields `here` and bits `without` with no candidate: `under` its
/// counted children's changes for that resolution summed, `left` how
/// many children it leaves to the binding above.
fn change(tile: Tile, here: Fields, without: u64, resolution: u8, under: i32, left: i32) -> i32 {
    let mask = MASK_BIT_WIDTH as i32;
    if tile.level > resolution {
        0
    } else if here.entirely_bound_at(resolution) {
        // Unmasked by the candidate: its mask bit and payload.
        without as i32 - mask - payload_bits(resolution - tile.level) as i32
    } else if resolution == tile.level + 1 && left > 0 {
        // A divide leaving children to the binding above, each bound
        // whole at the candidate's resolution: unmasked, a mask bit and
        // a payload bit each, and no flip bit or child mask.
        -mask + under + LEAVING_BITS - left * (mask + payload_bits(0) as i32)
    } else {
        -mask + under
    }
}

/// A held tile's change under a candidate of its own level's
/// resolution, which the costs pyramid does not hold: its children,
/// finer, change nothing there, and it leaves none to the binding above
/// then.
fn own_level_change(tile: Tile, here: Fields, without: u64) -> i32 {
    change(tile, here, without, tile.level, 0, 0)
}

impl CostPyramid {
    /// Counts, for every tile a count can reach from `roots`, all of one
    /// level, its bits and its changes under every candidate resolution,
    /// in a search area nested in `base`.
    pub fn fill(&mut self, complex_tiling: &ComplexTiling, bitmap: &Bitmap, roots: &[Tile], base: &NestedResolutions) {
        debug_assert!(roots.iter().all(|root| root.level == roots[0].level), "roots of one level");
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
    /// changes, whose fields are `here`.
    fn without_and_changes(&self, tile: Tile, here: Fields) -> (u64, Changes) {
        let without = self.counts.without(tile);
        let mut changes = self.counts.finer_changes(tile);
        changes[tile.level as usize - 1] = own_level_change(tile, here, without);
        (without, changes)
    }

    /// `tile`'s bits under a candidate of `resolution` in the area, whose
    /// fields are `here`.
    pub fn child_bits(&self, tile: Tile, here: Fields, resolution: u8) -> u64 {
        let without = self.counts.without(tile);
        let change = match resolution {
            NO_CANDIDATE => 0,
            own if own == tile.level => own_level_change(tile, here, without),
            coarser if coarser < tile.level => 0,
            finer => self.counts.change(tile, finer),
        };
        (without as i64 - change as i64) as u64
    }

    /// `tile`'s bits with no candidate above it, nested in the area's
    /// own nesting.
    pub fn without(&self, tile: Tile) -> u64 {
        self.counts.without(tile)
    }
}
