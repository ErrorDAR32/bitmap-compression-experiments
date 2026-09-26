//! Everything a region could say about itself.
//!
//! One list, asked for twice. The pass that runs first asks with
//! `encoded` false, because nothing has been written yet and a copy
//! has to be priced on whether the cells match rather than on whether
//! the decoder will have them. The descent asks with `encoded` true,
//! and then a copy is only offered where the decoder really will hold
//! what it copies from.
//!
//! What a region may leave out depends on what it is standing in.
//! A child left out of a mask is left to the closest binding above,
//! so a child may be left out exactly when it already reads what that
//! binding says. At the top of the bitmap that binding is nothing and
//! what it says is clear, which is the same rule with the same
//! answer.
//!
//! Nothing here chooses. It lays out the options and lets
//! [`super::cost`] price them.

use crate::dsrn::nesting::Knobs;
use crate::dsrn::nesting_data::{RegionCode, RegionMask, Standing, Workspace};
use crate::dsrn::region::{
    deepest_depth, same_cells, whole_region_encoded, Region, CHILD_COUNT, DIRECTIONS,
    EVERY_CHILD,
};
use crate::pyramid::{tile_of_bitmap, Pyramid};
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

/// Which children what is standing over them already gets right, and
/// so which a subdivision has to describe.
///
/// A child left out is left to the closest binding above, and that
/// binding is already going to put something there. Where it says one
/// thing over the whole region, a child that already reads it is
/// right. Where it covers the region with tiles, a child every one of
/// whose tiles is one thing is right, because a tile that is one
/// thing is a tile the binding can say with the bit it was going to
/// write anyway.
fn children_standing_gets_wrong(
    work: &Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    standing: Standing,
) -> RegionMask {
    let mut wrong = 0;
    for (at, child) in region.children().into_iter().enumerate() {
        let right = match standing {
            Standing::Clear => already_reads(pyramid, bitmap, child, false),
            Standing::Reads(reads) => already_reads(pyramid, bitmap, child, reads),
            Standing::Tiles(depth) => work.coarsest_depth_of(child) <= depth - 1,
            Standing::Nothing => false,
        };
        if !right {
            wrong |= 1 << at;
        }
    }
    RegionMask(wrong)
}

/// Whether a region already reads one thing all the way through.
fn already_reads(pyramid: &Pyramid, bitmap: &Bitmap, region: Region, reads: bool) -> bool {
    tile_of_bitmap(pyramid, bitmap, region.level, region.x, region.y) == Some(reads)
}

/// What a region's children stand in, once it has said what it says.
///
/// A binding covers them with its tiles, and a child that is one of
/// those tiles reads whichever of the two that tile is cheaper
/// saying. A copy leaves them nothing to stand in, because what it
/// puts there reads the neighbour rather than any one thing. A
/// subdivision says nothing of its own, so its children stand in
/// whatever it stands in.
pub fn standing_under(
    work: &Workspace,
    standing: Standing,
    region: Region,
    code: RegionCode,
    child: Region,
) -> Standing {
    match code {
        // One tile, and it is the whole region, so every child is
        // inside it.
        RegionCode::Bind { depth: 0, .. } => Standing::Reads(work.value_worth_standing(region)),
        RegionCode::Bind { depth, .. } => one_level_down(work, Standing::Tiles(depth), child),
        RegionCode::Copy { .. } => Standing::Nothing,
        RegionCode::Subdivide { .. } => one_level_down(work, standing, child),
    }
}

/// The same standing, read one level further down.
fn one_level_down(work: &Workspace, standing: Standing, child: Region) -> Standing {
    match standing {
        Standing::Tiles(1) => Standing::Reads(work.value_worth_standing(child)),
        Standing::Tiles(depth) => Standing::Tiles(depth - 1),
        held => held,
    }
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
    standing: Standing,
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
    if may_mask {
        let wrong = children_standing_gets_wrong(work, pyramid, bitmap, region, standing);
        if wrong != RegionMask::EVERY {
            ways.push(RegionCode::Subdivide { mask: wrong });
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
