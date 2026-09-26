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
    let under = depth - 1;
    let share = crate::dsrn::region::tiles_at_depth(under);
    let mut again = 0;
    for (at, child) in region.children().into_iter().enumerate() {
        if work.coarsest_depth_of(child) > under || work.cost_under_tiles_of(child, under) < share {
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
pub fn children_standing_gets_wrong(
    work: &Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    standing: Standing,
) -> RegionMask {
    let mut wrong = 0;
    for (at, child) in region.children().into_iter().enumerate() {
        let right = match standing {
            // One tile, and the children are inside it, so what is
            // standing over them is what that tile will say.
            Standing::Tiles(0) => {
                already_reads(pyramid, bitmap, child, work.value_worth_standing(region))
            }
            Standing::Tiles(depth) => work.coarsest_depth_of(child) <= depth - 1,
            Standing::Nothing => false,
            _ => already_reads(pyramid, bitmap, child, standing.reads() == Some(true)),
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
pub fn standing_under(work: &Workspace, standing: Standing, region: Region, code: RegionCode) -> Standing {
    match code {
        // One tile, and it is the whole region. A binding at one tile
        // describes no child again -- there is no mask to name one
        // with -- so nothing ever stands under it.
        RegionCode::Bind { depth: 0, .. } => Standing::Nothing,
        RegionCode::Bind { depth, .. } => Standing::Tiles(depth - 1),
        RegionCode::Copy { .. } => Standing::Nothing,
        // Neither of these covers what it does not name, so what it
        // does not name stands one level further into whatever the
        // region itself stands in.
        RegionCode::Subdivide { .. } | RegionCode::CopyEachChild { .. } => {
            standing_one_level_down(work, standing, region)
        }
    }
}

/// The same standing, read one level further in, for a region that
/// covers nothing of its own.
pub fn standing_one_level_down(
    work: &Workspace,
    standing: Standing,
    region: Region,
) -> Standing {
    match standing {
        Standing::Tiles(0) => Standing::Reads(work.value_worth_standing(region)),
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
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    encoded: bool,
) -> [Option<usize>; CHILD_COUNT] {
    let mut from = [None; CHILD_COUNT];
    for (at, child) in region.children().into_iter().enumerate() {
        // The pyramid has already asked whether any neighbour holds
        // the same cells, and where it says none there is no
        // direction to look in.
        if !pyramid.copyable(child.level, child.x, child.y) {
            continue;
        }
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
    let mut ways = every_way_of_binding(work, region, knobs);
    ways.extend(every_way_of_subdividing(work, pyramid, bitmap, region, knobs, standing));
    ways.extend(every_way_of_copying(work, bitmap, region, knobs, encoded));
    ways.extend(the_way_its_children_copy_themselves(
        work, pyramid, bitmap, region, knobs, encoded, standing,
    ));
    ways
}

/// The one thing only a 4x4 can say: that each of its children copies
/// from a neighbour of its own.
///
/// It can only be said when every child that is not already right can
/// copy, because a child it does not name is not described and there
/// is nowhere else for it to be said. A child that is already right
/// is left to the binding above, which was going to put it there in
/// any case.
pub fn the_way_its_children_copy_themselves(
    work: &Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    knobs: Knobs,
    encoded: bool,
    standing: Standing,
) -> Option<RegionCode> {
    if !knobs.four_by_four.may_copy_each_child(region) {
        return None;
    }
    let wrong = children_standing_gets_wrong(work, pyramid, bitmap, region, standing);
    if wrong == RegionMask::NONE {
        return None;
    }
    let from = where_each_child_copies_from(work, pyramid, bitmap, region, encoded);
    (0..CHILD_COUNT)
        .all(|at| !wrong.describes(at) || from[at].is_some())
        .then_some(RegionCode::CopyEachChild { mask: wrong })
}

/// The ways a region can cover the whole of itself: bind, or copy.
///
/// None of them depends on what the region is standing in. A binding
/// says every cell of its region and a copy takes every cell of one,
/// so what a binding above would have put there does not come into
/// it -- which is why the first pass can ask this once and then ask
/// about subdividing as many times as it has standings to try.
pub fn every_way_of_covering_it(
    work: &Workspace,
    bitmap: &Bitmap,
    region: Region,
    knobs: Knobs,
    encoded: bool,
) -> Vec<RegionCode> {
    let mut ways = every_way_of_binding(work, region, knobs);
    ways.extend(every_way_of_copying(work, bitmap, region, knobs, encoded));
    ways
}

/// The tile sizes a region could be bound at, and for each the
/// children a binding at that size would have to describe again.
///
/// The unmasked case needs no comparison at all: its header costs the
/// same at every depth and its payload is `tiles_at_depth(depth)`,
/// which only grows as the tiles get finer, so a deeper unmasked bind
/// can never be cheaper than the coarsest one that is still
/// homogeneous. That is true of every region on every bitmap, not
/// just the ones measured -- so there is exactly one unmasked
/// candidate, picked outright, not searched for.
///
/// Masked binds are not this simple. Which children a given depth has
/// to describe again genuinely changes from one depth to the next --
/// a coarser depth demands more of a child to be absorbed for free,
/// a finer one demands less but absorbs a smaller share per bit -- so
/// there is no depth that dominates every other, and every one of
/// them still has to be priced.
fn every_way_of_binding(work: &Workspace, region: Region, knobs: Knobs) -> Vec<RegionCode> {
    let mut ways =
        vec![RegionCode::Bind { level: region.level, depth: work.coarsest_depth_of(region), mask: RegionMask::NONE }];

    if knobs.masking.allows(region) {
        for depth in 1..=deepest_depth(region.level) {
            let mask = described_again_by_a_binding(work, region, depth);
            // A mask naming all four children describes exactly what
            // an unmasked subdivide describes, for strictly more
            // header. It is not, as it first looks, priced the same
            // as subdividing: it hands its children `Tiles(depth-1)`
            // instead of whatever standing this region was itself
            // given, and that can look cheaper to the argmin than it
            // turns out to be once the children are actually written
            // -- measured worse on every knob setting tried, never
            // better. So it is excluded outright rather than left for
            // the argmin to occasionally get wrong.
            if mask != RegionMask::NONE && mask != RegionMask::EVERY {
                ways.push(RegionCode::Bind { level: region.level, depth, mask });
            }
        }
    }
    ways
}

/// The neighbours a region could be copied from, and for each the
/// children that copy would get wrong.
fn every_way_of_copying(
    work: &Workspace,
    bitmap: &Bitmap,
    region: Region,
    knobs: Knobs,
    encoded: bool,
) -> Vec<RegionCode> {
    let mut ways = Vec::new();
    let may_mask = knobs.masking.allows(region);

    for direction in 0..DIRECTIONS.len() {
        let Some(from) = region.neighbour(direction) else { continue };
        let missed = children_the_copy_misses(work, bitmap, region, from, encoded);
        // Four children each holding what the neighbour's does is the
        // whole region holding what the neighbour does, so there is
        // nothing further to ask.
        if missed == RegionMask::NONE
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

/// The ways a region can say nothing of its own and leave it to its
/// children: all four of them, or only the ones what is standing over
/// it gets wrong.
///
/// This is the only thing a region could say that depends on what it
/// is standing in, which is why it is on its own.
pub fn every_way_of_subdividing(
    work: &Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    knobs: Knobs,
    standing: Standing,
) -> impl Iterator<Item = RegionCode> {
    let mut only_the_wrong_ones = None;
    if knobs.masking.allows(region) {
        let wrong = children_standing_gets_wrong(work, pyramid, bitmap, region, standing);
        if wrong != RegionMask::EVERY {
            only_the_wrong_ones = Some(RegionCode::Subdivide { mask: wrong });
        }
    }
    [Some(RegionCode::Subdivide { mask: RegionMask::EVERY }), only_the_wrong_ones]
        .into_iter()
        .flatten()
}
