//! Everything a region could say about itself.
//!
//! One list, asked for twice. The pass that runs first asks with
//! `encoded` false, because nothing has been written yet and a copy
//! has to be priced on whether the cells match rather than on whether
//! the decoder will have them. The descent asks with `encoded` true,
//! and then a copy is only offered where the decoder really will hold
//! what it copies from.
//!
//! Nothing here chooses. It lays out the options and lets
//! [`super::cost`] price them.

use crate::dsrn::nesting::Masking;
use crate::dsrn::nesting_data::{RegionCode, RegionMask, Workspace};
use crate::dsrn::region::{
    all_cells_clear, deepest_depth, same_cells, whole_region_encoded, Region, CHILD_COUNT,
    DIRECTIONS, EVERY_CHILD,
};
use crate::pyramid::Pyramid;
use crate::Bitmap;

/// Which children a binding at a tile size can keep, and which it has
/// to hand down.
///
/// A child whose tiles are not all homogeneous at this size has to go
/// down, or its share of the payload would be a lie. A child cheaper
/// to describe than its share wants to go down.
fn kept_by_a_binding(work: &Workspace, region: Region, depth: usize) -> RegionMask {
    let share = crate::dsrn::region::tiles_at_depth(depth - 1);
    let mut kept = 0;
    for (at, child) in region.children().into_iter().enumerate() {
        if work.coarsest_depth_of(child) <= depth - 1 && work.cost_of(child) >= share {
            kept |= 1 << at;
        }
    }
    RegionMask(kept)
}

/// Which children hold nothing at all, and so can be left clear.
fn empty_children(pyramid: &Pyramid, bitmap: &Bitmap, region: Region) -> RegionMask {
    let mut empty = 0;
    for (at, child) in region.children().into_iter().enumerate() {
        if all_cells_clear(pyramid, bitmap, child) {
            empty |= 1 << at;
        }
    }
    RegionMask(empty)
}

/// Which children match the neighbour's, in a direction.
fn children_matching(
    work: &Workspace,
    bitmap: &Bitmap,
    region: Region,
    from: Region,
    encoded: bool,
) -> RegionMask {
    let mut matching = 0;
    let theirs = from.children();
    for (at, mine) in region.children().into_iter().enumerate() {
        let there = theirs[at];
        if same_cells(bitmap, mine, there)
            && (!encoded || whole_region_encoded(&work.encoded_cells, there))
        {
            matching |= 1 << at;
        }
    }
    RegionMask(matching)
}

/// Every description this region could give of itself.
pub fn every_description(
    work: &Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    masking: Masking,
    encoded: bool,
) -> Vec<RegionCode> {
    let mut ways = Vec::new();
    let may_mask = masking.allows(region);
    let level = region.level;

    for depth in 0..=deepest_depth(level) {
        // Unmasked: every tile of the region has to be homogeneous.
        if depth >= work.coarsest_depth_of(region) {
            ways.push(RegionCode::Bind { level, depth, mask: RegionMask::EVERY });
        }
        // Masked: only the children it keeps have to be, and a mask
        // names children, so there have to be children to name.
        if depth >= 1 && may_mask {
            let mask = kept_by_a_binding(work, region, depth);
            if mask != RegionMask::EVERY {
                ways.push(RegionCode::Bind { level, depth, mask });
            }
        }
    }

    ways.push(RegionCode::Subdivide { mask: RegionMask::NONE });
    if may_mask {
        let empty = empty_children(pyramid, bitmap, region);
        if empty != RegionMask::NONE && empty != RegionMask::EVERY {
            ways.push(RegionCode::Subdivide { mask: empty });
        }
    }

    for direction in 0..DIRECTIONS.len() {
        let Some(from) = region.neighbour(direction) else { continue };
        let matching = children_matching(work, bitmap, region, from, encoded);
        if matching == RegionMask::EVERY
            && same_cells(bitmap, region, from)
            && (!encoded || whole_region_encoded(&work.encoded_cells, from))
        {
            ways.push(RegionCode::Copy { direction, mask: RegionMask::EVERY });
            continue;
        }
        if may_mask && matching != RegionMask::NONE {
            ways.push(RegionCode::Copy { direction, mask: matching });
        }
    }

    let _ = (CHILD_COUNT, EVERY_CHILD);
    ways
}
