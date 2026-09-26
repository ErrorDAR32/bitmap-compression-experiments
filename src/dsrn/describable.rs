//! Everything a region could say about itself.
//!
//! One list, asked for twice. The pass that runs first asks with
//! `encoded` false, because nothing has been written yet and a copy
//! has to be priced on whether the cells match rather than on whether
//! the decoder will have them. The descent asks with `encoded` true,
//! and then a copy is only offered where the decoder really will hold
//! what it copies from.
//!
//! `covered_from_above` is the other thing the descent knows and the
//! first pass does not: whether some binding or copy already covers
//! this region, and so will fill whatever is left out here. Where
//! nothing does, what is left out stays clear, and only a child that
//! is already clear may be left out.
//!
//! Nothing here chooses. It lays out the options and lets
//! [`super::cost`] price them.

use crate::dsrn::nesting::Knobs;
use crate::dsrn::nesting_data::{RegionCode, RegionMask, Workspace};
use crate::dsrn::region::{
    all_cells_clear, deepest_depth, same_cells, whole_region_encoded, Region, CHILD_COUNT,
    DIRECTIONS, EVERY_CHILD,
};
use crate::pyramid::Pyramid;
use crate::Bitmap;

/// Which children a binding at a tile size has to describe again,
/// and which it can simply cover.
///
/// A child whose tiles are not all homogeneous at this size has to be
/// described again, or the bits the binding wrote over it would be a
/// lie. A child cheaper to describe than the bits covering it costs
/// wants to be described again too.
fn described_again_by_a_binding(work: &Workspace, region: Region, depth: usize) -> RegionMask {
    let share = crate::dsrn::region::tiles_at_depth(depth - 1);
    let mut again = 0;
    for (at, child) in region.children().into_iter().enumerate() {
        if work.coarsest_depth_of(child) > depth - 1 || work.cost_of(child) < share {
            again |= 1 << at;
        }
    }
    RegionMask(again)
}

/// Which children a subdivision has to describe when nothing above
/// covers the region: every child that holds anything.
///
/// A child left out is left to the closest binding above. Where there
/// is none, what is left out stays as the decoder found it, which is
/// clear -- so only a child that is clear may be left out.
fn children_holding_anything(pyramid: &Pyramid, bitmap: &Bitmap, region: Region) -> RegionMask {
    let mut holding = 0;
    for (at, child) in region.children().into_iter().enumerate() {
        if !all_cells_clear(pyramid, bitmap, child) {
            holding |= 1 << at;
        }
    }
    RegionMask(holding)
}

/// Which children a copy would get wrong, in a direction, and so has
/// to describe again.
fn children_the_copy_misses(
    work: &Workspace,
    bitmap: &Bitmap,
    region: Region,
    from: Region,
    encoded: bool,
) -> RegionMask {
    let mut missed = 0;
    let theirs = from.children();
    for (at, mine) in region.children().into_iter().enumerate() {
        let there = theirs[at];
        if !(same_cells(bitmap, mine, there)
            && (!encoded || whole_region_encoded(&work.encoded_cells, there)))
        {
            missed |= 1 << at;
        }
    }
    RegionMask(missed)
}

/// Which direction each child of a 4x4 can copy from, if any.
///
/// A child looks at its own four neighbours, not at the neighbours of
/// the region above it: a 2x2 copying from the 2x2 beside it is the
/// same question a region of any size asks, only asked of a child
/// that will never be a region in its own right.
pub fn where_each_child_copies_from(
    work: &Workspace,
    bitmap: &Bitmap,
    region: Region,
    encoded: bool,
) -> [Option<usize>; CHILD_COUNT] {
    let mut from = [None; CHILD_COUNT];
    for (at, child) in region.children().into_iter().enumerate() {
        from[at] = (0..DIRECTIONS.len()).find(|&direction| {
            child.neighbour(direction).is_some_and(|beside| {
                same_cells(bitmap, child, beside)
                    && (!encoded || whole_region_encoded(&work.encoded_cells, beside))
            })
        });
    }
    from
}

/// Every description this region could give of itself.
pub fn every_description(
    work: &Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    knobs: Knobs,
    encoded: bool,
    covered_from_above: bool,
) -> Vec<RegionCode> {
    let mut ways = Vec::new();
    let may_mask = knobs.masking.allows(region);
    let level = region.level;

    for depth in 0..=deepest_depth(level) {
        // Unmasked: every tile of the region has to be homogeneous,
        // because the binding covers all of it on its own.
        if depth >= work.coarsest_depth_of(region) {
            ways.push(RegionCode::Bind { level, depth, mask: RegionMask::NONE });
        }
        // Masked: only the children it covers have to be, and a mask
        // names children, so there have to be children to name.
        if depth >= 1 && may_mask {
            let mask = described_again_by_a_binding(work, region, depth);
            if mask != RegionMask::NONE {
                ways.push(RegionCode::Bind { level, depth, mask });
            }
        }
    }

    ways.push(RegionCode::Subdivide { mask: RegionMask::EVERY });
    if may_mask && !covered_from_above {
        let holding = children_holding_anything(pyramid, bitmap, region);
        if holding != RegionMask::EVERY {
            ways.push(RegionCode::Subdivide { mask: holding });
        }
    }

    for direction in 0..DIRECTIONS.len() {
        let Some(from) = region.neighbour(direction) else { continue };
        let missed = children_the_copy_misses(work, bitmap, region, from, encoded);
        if missed == RegionMask::NONE
            && same_cells(bitmap, region, from)
            && (!encoded || whole_region_encoded(&work.encoded_cells, from))
        {
            ways.push(RegionCode::Copy { direction, mask: RegionMask::NONE });
            continue;
        }
        if may_mask && missed != RegionMask::EVERY {
            ways.push(RegionCode::Copy { direction, mask: missed });
        }
    }

    let _ = (CHILD_COUNT, EVERY_CHILD);
    ways
}
