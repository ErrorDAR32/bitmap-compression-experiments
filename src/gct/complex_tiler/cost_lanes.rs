//! Cost lanes: every bit count the complex tiler asks for in one search
//! area, counted once before any is asked, and held -- memory for the
//! counting it saves.
//!
//! Lane 0 holds each tile's bits nested as the area is; lane `r` its
//! bits with one more complex tile, of resolution `r`, above it -- the
//! candidate being scored. A tile's bits depend only on the complex
//! tiles able to unmask it or anything under it, those of resolution its
//! level or finer, so a tile has lanes only for those; coarser, lane `r`
//! is lane 0.
//!
//! Filled in two walks: down from the area's search roots, collecting
//! the tiles a count can reach -- all a tile placed nothing says, the
//! children a masking tile masks, nothing under a tile placed whole --
//! with the value bound above each, and the lanes some candidate above
//! it will ask for; then back up, a level at a time,
//! each tile's lanes from its children's, by the rules the bit count
//! ([`super::bit_cost`]) spells out. 2x2s are counted as they are asked
//! for, which takes a few steps. Nothing is changed while the lanes are
//! read: they are a snapshot of the tiling as the pass found it.

use super::bit_cost::node_bits;
use super::complex_tile_candidates::tried_resolutions;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::pyramids::placements::Placement;
use crate::gct::pyramids::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{Tile, CELL_LEVEL};
use crate::Bitmap;

/// The finest level held: 4x4, the finest candidate. A 2x2 is counted
/// when asked for.
const FINEST_HELD: u8 = CELL_LEVEL - 2;

/// Enough for any tile's bits, the whole bitmap's included.
const LANE_SHAPE: PyramidShape = PyramidShape { coarsest_level: 0, finest_level: FINEST_HELD, element_bits: 32 };

/// Lanes: 0 for the area's own nesting, then one a resolution.
const LANES: usize = CELL_LEVEL as usize + 1;
/// Lane 0's bit in a set of lanes: every tile holds it.
const LANE_0: u16 = 1;

/// The lanes, and room to collect the tiles to fill them for.
pub struct CostLanes {
    /// Each lane's bits, by tile.
    lanes: Vec<Pyramid>,
    /// The tiles a count can reach, by level, each with the value bound
    /// above it and the lanes it must hold, bit `r` for lane `r`.
    reached: Vec<Vec<(Tile, bool, u16)>>,
}

impl Default for CostLanes {
    /// Every lane allocated, nothing counted.
    fn default() -> Self {
        Self {
            lanes: (0..LANES).map(|_| Pyramid::new(LANE_SHAPE)).collect(),
            reached: vec![Vec::new(); FINEST_HELD as usize + 1],
        }
    }
}

/// The lane a tile of `level` reads for lane `lane`: `lane`'s own if its
/// resolution can reach the tile, else lane 0.
fn lane_at(level: u8, lane: u8) -> u8 {
    if lane as usize != 0 && lane >= level {
        lane
    } else {
        0
    }
}

/// The nesting lane `lane` counts in: `base`, and for a lane other than
/// 0, a complex tile of its resolution inside it.
fn nesting_of(base: &NestedResolutions, lane: u8) -> NestedResolutions {
    if lane == 0 {
        *base
    } else {
        base.with_nested(lane)
    }
}

impl CostLanes {
    /// Counts every lane for every tile a count can reach from `roots`,
    /// each with the value bound above it, in a search area nested in
    /// `base`.
    pub fn fill(&mut self, complex_tiling: &Pyramid, bitmap: &Bitmap, roots: impl Iterator<Item = (Tile, bool)>, base: &NestedResolutions) {
        for level in self.reached.iter_mut() {
            level.clear();
        }
        let mut coarsest = FINEST_HELD + 1;
        for (root, bound_above) in roots {
            if root.level <= FINEST_HELD {
                coarsest = coarsest.min(root.level);
                self.reached[root.level as usize].push((root, bound_above, LANE_0));
            }
        }
        for level in coarsest..FINEST_HELD {
            let (these, finer) = self.reached.split_at_mut(level as usize + 1);
            for &(tile, bound_above, lanes) in &these[level as usize] {
                let here = complex_tiling.fields(tile);
                let (reached, inside, tried) = match here.placed() {
                    None if base.unmasking(here, tile).is_none() => {
                        let tried = tried_resolutions(here, tile.level, base).fold(0, |tried, lane| tried | 1 << lane);
                        (0b1111, bound_above, tried)
                    }
                    None => (0b1111, bound_above, 0),
                    Some(Placement::Bound { value, masked_children }) => (masked_children, value, 0),
                    Some(Placement::Copied { masked_children, .. }) => (masked_children, bound_above, 0),
                };
                // A child holds lane 0, and every lane its parent holds or
                // tries that can still reach it.
                let child_lanes = LANE_0 | (lanes | tried) & !((1 << (level + 1)) - 1);
                for child in tile.children() {
                    if reached & 1 << child.child_index() != 0 {
                        finer[0].push((child, inside, child_lanes));
                    }
                }
            }
        }
        for level in (coarsest..=FINEST_HELD).rev() {
            for at in 0..self.reached[level as usize].len() {
                let (tile, bound_above, lanes) = self.reached[level as usize][at];
                for lane in (0..LANES as u8).filter(|&lane| lanes & 1 << lane != 0) {
                    let mut nested = nesting_of(base, lane);
                    let here = complex_tiling.fields(tile);
                    let bits = node_bits(complex_tiling, bitmap, tile, here, &mut nested, bound_above, &mut |child, nested, bound_above| {
                        self.child_bits(complex_tiling, bitmap, child, nested, bound_above, lane)
                    });
                    self.lanes[lane as usize].set(tile, bits);
                }
            }
        }
    }

    /// `tile`'s bits in lane `lane`, nested as `nested` says: held, or,
    /// for a 2x2, counted now.
    pub fn child_bits(
        &self,
        complex_tiling: &Pyramid,
        bitmap: &Bitmap,
        tile: Tile,
        nested: &mut NestedResolutions,
        bound_above: bool,
        lane: u8,
    ) -> u64 {
        if tile.level > FINEST_HELD {
            let here = complex_tiling.fields(tile);
            return node_bits(complex_tiling, bitmap, tile, here, nested, bound_above, &mut |_, _, _| {
                unreachable!("a 2x2 has no child nodes")
            });
        }
        self.lanes[lane_at(tile.level, lane) as usize].get(tile)
    }

    /// `tile`'s bits nested in the area's own nesting.
    pub fn without(&self, tile: Tile) -> u64 {
        self.lanes[0].get(tile)
    }
}
