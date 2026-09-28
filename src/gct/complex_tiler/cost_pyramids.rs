//! Cost pyramids: every bit count the complex tiler asks for in one
//! search area, counted once before any is asked, and held -- memory for
//! the counting it saves.
//!
//! An array of pyramids, indexed by resolution: pyramid 0 holds each
//! tile's bits nested as the area is, with no candidate above it;
//! pyramid `r` its bits with one more complex tile, of resolution `r`,
//! above it -- the candidate being scored. A tile's bits depend only on
//! the complex tiles able to unmask it or anything under it, those of
//! resolution its level or finer, so a tile is held in pyramid `r` only
//! for those; for a coarser `r`, pyramid 0 is read instead.
//!
//! Filled in two walks: down from the area's search roots, collecting
//! the tiles a count can reach -- all a tile placed nothing says, the
//! children a masking tile masks, nothing under a tile placed whole --
//! with the resolutions some candidate above each will ask for; then
//! back up, a level at a time, each tile's bits from its children's, by
//! the rules the bit count ([`super::bit_cost`]) spells out. The value
//! bound above a tile is its own field in the complex tiling, not
//! carried down. 2x2s are counted as they are asked for, which takes a
//! few steps. Nothing is changed while the pyramids are read: they are a
//! snapshot of the tiling as the pass found it.

use super::bit_cost::node_bits;
use super::complex_tile_candidates::tried_resolutions;
use crate::fixed_list::FixedList;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::pyramids::placements::Placement;
use crate::gct::pyramids::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{tiles_across, Tile, CELL_LEVEL};
use crate::Bitmap;

/// The finest level held: 4x4, the finest candidate. A 2x2 is counted
/// when asked for.
const FINEST_HELD: u8 = CELL_LEVEL - 2;

/// Enough for any tile's bits, the whole bitmap's included.
const COST_SHAPE: PyramidShape = PyramidShape { coarsest_level: 0, finest_level: FINEST_HELD, element_bits: 32 };

/// One pyramid for no candidate, then one a resolution.
const PYRAMIDS: usize = CELL_LEVEL as usize + 1;
/// The resolution index meaning no candidate above.
const NO_CANDIDATE: u8 = 0;
/// [`NO_CANDIDATE`]'s bit in a set of resolutions: every tile holds it.
const NO_CANDIDATE_BIT: u16 = 1 << NO_CANDIDATE;

/// The most tiles of one level a count can reach: every tile of the
/// finest level held.
const MOST_REACHED: usize = tiles_across(FINEST_HELD) * tiles_across(FINEST_HELD);

/// The cost pyramids, and room to collect the tiles to fill them for,
/// all allocated once.
pub struct CostPyramids {
    /// Each tile's bits, by the resolution of the candidate above it.
    pyramids: [Pyramid; PYRAMIDS],
    /// The tiles a count can reach, by level, each with the resolutions
    /// it must be held for, bit `r` for resolution `r`.
    reached: [FixedList<(Tile, u16), MOST_REACHED>; FINEST_HELD as usize + 1],
}

impl Default for CostPyramids {
    /// Every pyramid allocated, nothing counted.
    fn default() -> Self {
        Self { pyramids: std::array::from_fn(|_| Pyramid::new(COST_SHAPE)), reached: std::array::from_fn(|_| FixedList::new()) }
    }
}

/// The pyramid a tile of `level` is read from for a candidate of
/// `resolution` above: that resolution's own if it can reach the tile,
/// else [`NO_CANDIDATE`]'s.
fn resolution_at(level: u8, resolution: u8) -> u8 {
    if resolution != NO_CANDIDATE && resolution >= level {
        resolution
    } else {
        NO_CANDIDATE
    }
}

/// The nesting a candidate of `resolution` counts its tiles in: `base`,
/// and for a candidate, a complex tile of its resolution inside it.
fn nesting_of(base: &NestedResolutions, resolution: u8) -> NestedResolutions {
    if resolution == NO_CANDIDATE {
        *base
    } else {
        base.with_nested(resolution)
    }
}

impl CostPyramids {
    /// Counts, for every tile a count can reach from `roots`, its bits
    /// under every candidate resolution asked for, in a search area
    /// nested in `base`.
    pub fn fill(&mut self, complex_tiling: &Pyramid, bitmap: &Bitmap, roots: impl Iterator<Item = Tile>, base: &NestedResolutions) {
        for level in self.reached.iter_mut() {
            level.clear();
        }
        let mut coarsest = FINEST_HELD + 1;
        for root in roots {
            if root.level <= FINEST_HELD {
                coarsest = coarsest.min(root.level);
                self.reached[root.level as usize].push((root, NO_CANDIDATE_BIT));
            }
        }
        for level in coarsest..FINEST_HELD {
            let (these, finer) = self.reached.split_at_mut(level as usize + 1);
            for &(tile, resolutions) in &these[level as usize] {
                let here = complex_tiling.fields(tile);
                let (reached, tried) = match here.placed() {
                    None if base.unmasking(here, tile).is_none() => {
                        let tried = tried_resolutions(here, tile.level, base).fold(0, |tried, resolution| tried | 1 << resolution);
                        (0b1111, tried)
                    }
                    None => (0b1111, 0),
                    Some(Placement::Bound { masked_children, .. } | Placement::Copied { masked_children, .. }) => (masked_children, 0),
                };
                // A child is held for no candidate, and for every
                // resolution its parent is held for or tries that can
                // still reach it.
                let child_resolutions = NO_CANDIDATE_BIT | (resolutions | tried) & !((1 << (level + 1)) - 1);
                for child in tile.children() {
                    if reached & 1 << child.child_index() != 0 {
                        finer[0].push((child, child_resolutions));
                    }
                }
            }
        }
        for level in (coarsest..=FINEST_HELD).rev() {
            for at in 0..self.reached[level as usize].len() {
                let (tile, resolutions) = self.reached[level as usize][at];
                let here = complex_tiling.fields(tile);
                for resolution in (0..PYRAMIDS as u8).filter(|&resolution| resolutions & 1 << resolution != 0) {
                    let mut nested = nesting_of(base, resolution);
                    let bits = node_bits(complex_tiling, bitmap, tile, here, &mut nested, here.bound_above(), &mut |child, nested, _| {
                        self.child_bits(complex_tiling, bitmap, child, nested, resolution)
                    });
                    self.pyramids[resolution as usize].set(tile, bits);
                }
            }
        }
    }

    /// `tile`'s bits under a candidate of `resolution`, nested as
    /// `nested` says: held, or, for a 2x2, counted now.
    pub fn child_bits(&self, complex_tiling: &Pyramid, bitmap: &Bitmap, tile: Tile, nested: &mut NestedResolutions, resolution: u8) -> u64 {
        if tile.level > FINEST_HELD {
            let here = complex_tiling.fields(tile);
            return node_bits(complex_tiling, bitmap, tile, here, nested, here.bound_above(), &mut |_, _, _| {
                unreachable!("a 2x2 has no child nodes")
            });
        }
        self.pyramids[resolution_at(tile.level, resolution) as usize].get(tile)
    }

    /// `tile`'s bits with no candidate above it, nested in the area's
    /// own nesting.
    pub fn without(&self, tile: Tile) -> u64 {
        self.pyramids[NO_CANDIDATE as usize].get(tile)
    }
}
