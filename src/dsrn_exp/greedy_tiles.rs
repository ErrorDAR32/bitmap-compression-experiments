//! An alternate algorithm to DSRN's own: a flat, size-ordered greedy
//! tile placement, with no region tree at all.
//!
//! DSRN decides a region's whole subtree at once, comparing the exact
//! cost of every way it could go before committing to any of it. This
//! is the opposite kind of pass: there are no regions, no standing, no
//! recursive cost tables. There is a plane and a size, biggest first,
//! and one rule at each size: a tile that is one thing, or that
//! matches a same-size neighbour reading order puts before it and
//! that neighbour is already decided, gets placed and claimed; a tile
//! that is neither is left for the next, finer size to try on its own
//! four quarters. Tiles never overlap, so a size that has already
//! claimed a tile is a size no finer pass ever has to look at again,
//! and by the time the pass reaches 1x1 every remaining cell is, on
//! its own, one thing -- so the pass always finishes and always
//! covers the whole bitmap.
//!
//! This file only decides which tiles that rule would place. It does
//! not write a bitstream: there is nothing here yet that says how a
//! tile's own size, position and kind (bound to a value, or copying a
//! direction) would be spelled out in bits, only how many of each
//! kind there would be. That is the thing worth counting first,
//! against what the region-based encoder actually produces, before
//! anything is spent on a grammar for it.

use crate::dsrn::region::{same_cells, Region, DIRECTIONS};
use crate::pyramid::{tile_of_bitmap, tiles_across, Pyramid, CELL_LEVEL};
use crate::Bitmap;

/// How many tiles the greedy pass placed, by level, and how many of
/// those were copies rather than a bound value.
///
/// Level 0 is the whole bitmap as one tile; level [`CELL_LEVEL`] is a
/// single cell. A tile at level `CELL_LEVEL` is never a copy -- there
/// is nothing smaller for its one cell to say but its own bit.
#[derive(Default, Clone, Copy)]
pub struct GreedyTileCounts {
    pub bound_at_level: [usize; CELL_LEVEL + 1],
    pub copied_at_level: [usize; CELL_LEVEL + 1],
}

impl GreedyTileCounts {
    /// Every tile the pass placed, whatever its size or kind.
    pub fn total_tiles(&self) -> usize {
        self.bound_at_level.iter().sum::<usize>() + self.copied_at_level.iter().sum::<usize>()
    }

    /// Every tile that copied rather than carried its own value.
    pub fn total_copies(&self) -> usize {
        self.copied_at_level.iter().sum()
    }
}

/// Runs the greedy pass over one bitmap.
pub fn greedy_tile_pass(pyramid: &Pyramid, bitmap: &Bitmap) -> GreedyTileCounts {
    let mut claimed = Bitmap::new();
    let mut counts = GreedyTileCounts::default();

    for level in 0..=CELL_LEVEL {
        let across = tiles_across(level);
        for y in 0..across {
            for x in 0..across {
                let tile = Region { level, x, y };
                let (cx, cy) = tile.top_left_cell();

                // Tiles are placed biggest first and never overlap,
                // so if this tile's corner is already claimed, a
                // coarser tile placed earlier covers the whole of it
                // -- same-size tiles partition the plane, and nothing
                // smaller has run yet.
                if claimed.get(cx as u8, cy as u8) {
                    continue;
                }

                if tile_of_bitmap(pyramid, bitmap, level, x, y).is_some() {
                    counts.bound_at_level[level] += 1;
                } else if level < CELL_LEVEL
                    && (0..DIRECTIONS.len())
                        .any(|direction| copies_from(&claimed, bitmap, tile, direction))
                {
                    counts.copied_at_level[level] += 1;
                } else {
                    // Neither one thing nor a copy of anything already
                    // decided: left for this tile's four quarters, one
                    // level finer, to each try for themselves.
                    continue;
                }
                claim(&mut claimed, tile);
            }
        }
    }
    counts
}

/// Whether a tile copies a same-size neighbour in a direction: the
/// neighbour must already be fully decided, and hold the same cells.
///
/// "Already decided" is one cell, not the whole neighbour: every
/// claim this pass ever makes covers a whole tile aligned to some
/// level, and every level's grid refines the one before it, so any
/// already-placed tile that reaches the neighbour's corner is either
/// the neighbour itself, already claimed whole, or a coarser tile
/// that contains the whole of it. There is no way for a claim to
/// cover only part of it.
fn copies_from(claimed: &Bitmap, bitmap: &Bitmap, tile: Region, direction: usize) -> bool {
    tile.neighbour(direction).is_some_and(|beside| {
        let (bx, by) = beside.top_left_cell();
        claimed.get(bx as u8, by as u8) && same_cells(bitmap, tile, beside)
    })
}

/// Marks every cell of a tile claimed.
fn claim(claimed: &mut Bitmap, tile: Region) {
    let (x, y) = tile.top_left_cell();
    let side = tile.side_in_cells();
    claimed.set_rect(x as i64, y as i64, (x + side - 1) as i64, (y + side - 1) as i64);
}


/// Compares the greedy pass against the region-based encoder at its
/// best known setting, in the one currency both understand: how many
/// payload-bearing tiles each produces, and at what sizes.
pub fn run() {
    use crate::dsrn::{encode, FourByFour, Knobs, Masking, Workspace};
    use crate::samples;
    use crate::table::Table;

    let knobs = Knobs { masking: Masking::Anywhere, four_by_four: FourByFour::ItsOwnGrammar };
    let (mut pyramid, mut work, mut out) =
        (Pyramid::new(), Workspace::new(), crate::dsrn::Encoded::default());

    for (family, maps) in samples::every_family() {
        let (mut dsrn_bound, mut dsrn_copied) = ([0usize; CELL_LEVEL + 1], [0usize; CELL_LEVEL + 1]);
        let (mut greedy_bound, mut greedy_copied) =
            ([0usize; CELL_LEVEL + 1], [0usize; CELL_LEVEL + 1]);

        for bitmap in &maps {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            encode(&pyramid, bitmap, knobs, &mut work, &mut out);
            for level in 0..=CELL_LEVEL {
                dsrn_bound[level] += out.counts.bound_tiles_at_level[level];
                dsrn_copied[level] += out.counts.copied_tiles_at_level[level];
            }

            let greedy = greedy_tile_pass(&pyramid, bitmap);
            for level in 0..=CELL_LEVEL {
                greedy_bound[level] += greedy.bound_at_level[level];
                greedy_copied[level] += greedy.copied_at_level[level];
            }
        }

        let n = maps.len();
        println!("
  {family}, {n} bitmaps. tiles a bitmap, by size.
");
        let mut t = Table::new(&[
            "tile side
in cells",
            "dsrn
bound",
            "dsrn
copied",
            "greedy
bound",
            "greedy
copied",
        ]);
        let (mut dsrn_total, mut greedy_total) = (0usize, 0usize);
        for level in 0..=CELL_LEVEL {
            let side = crate::pyramid::tile_side(level);
            t.row(&[
                format!("{side}x{side}"),
                (dsrn_bound[level] / n).to_string(),
                (dsrn_copied[level] / n).to_string(),
                (greedy_bound[level] / n).to_string(),
                (greedy_copied[level] / n).to_string(),
            ]);
            dsrn_total += dsrn_bound[level] + dsrn_copied[level];
            greedy_total += greedy_bound[level] + greedy_copied[level];
        }
        t.print();
        println!(
            "
  total tiles a bitmap: dsrn {}, greedy {} ({:+.1}%)",
            dsrn_total / n,
            greedy_total / n,
            100.0 * (greedy_total as f64 - dsrn_total as f64) / dsrn_total as f64
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples;

    /// Every tile the pass places covers area that adds up to exactly
    /// the whole bitmap, once -- no gaps, and the "claimed" check
    /// already rules out overlaps.
    #[test]
    fn covers_every_cell_exactly_once() {
        let mut pyramid = Pyramid::new();
        for (_, maps) in samples::every_family() {
            for bitmap in &maps {
                pyramid.clear();
                pyramid.rebuild(bitmap);
                let counts = greedy_tile_pass(&pyramid, bitmap);
                let area: usize = (0..=CELL_LEVEL)
                    .map(|level| {
                        let side = crate::pyramid::tile_side(level);
                        (counts.bound_at_level[level] + counts.copied_at_level[level])
                            * side
                            * side
                    })
                    .sum();
                assert_eq!(area, 256 * 256, "left cells uncovered or double-covered");
            }
        }
    }

    /// A same-size neighbour is only ever a copy source once it is
    /// homogeneous itself -- under this pass's strict biggest-first,
    /// one-size-at-a-time order, a heterogeneous tile is never
    /// resolved before its same-size neighbours are looked at, so
    /// there is nothing yet for them to match against. This nails
    /// that down so a future change to the ordering has to notice it
    /// broke, rather than silently stop exercising copy at all.
    #[test]
    fn copy_never_fires_under_strict_size_major_order() {
        let mut pyramid = Pyramid::new();
        let mut total_copies = 0usize;
        for (_, maps) in samples::every_family() {
            for bitmap in &maps {
                pyramid.clear();
                pyramid.rebuild(bitmap);
                total_copies += greedy_tile_pass(&pyramid, bitmap).total_copies();
            }
        }
        assert_eq!(total_copies, 0, "copy fired under an ordering that should never let it");
    }
}
