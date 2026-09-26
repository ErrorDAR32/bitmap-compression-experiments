//! What a description costs, in bits.
//!
//! Two numbers, and keeping them apart is what lets an encode be
//! checked against itself. [`description_tree_size`] is what a region
//! spends saying what it is; its payload is spent a bit at a time as
//! the descent finds out what is left of its tiles, and the two added
//! up over every region described must come to exactly what the
//! encode wrote. [`whole_subtree_size`] is what a description is
//! worth to the region above it, which is all of that for the whole
//! subtree and the binding bits nobody below took.

use crate::dsrn::describable::standing_under;
use crate::dsrn::nesting_data::{
    RegionCode, Standing, Workspace, CHILD_MASK_WIDTH, CODE_WIDTH, DIRECTION_WIDTH,
    FINEST_LEVEL_WITH_A_GRAMMAR,
};
use crate::dsrn::region::{deepest_depth, tiles_at_depth, Region, CHILD_COUNT};
use crate::pyramid::CELL_LEVEL;

/// The width of a binding's tile size field, for a region at `level`.
///
/// A region can name every size from itself down to its cells, which
/// is `deepest_depth(level) + 1` of them, so it spends exactly the
/// bits those need: four at the whole bitmap, none at a cell. A flat
/// field wide enough for the whole bitmap would charge a 4x4 four bits
/// to say what two can.
pub fn tile_size_field_width(level: usize) -> usize {
    let mut width = 0;
    while (1 << width) < deepest_depth(level) + 1 {
        width += 1;
    }
    width
}

/// What a description spends on itself, in the tree.
///
/// A binding's payload is the rest of what it spends, and how much of
/// it there is depends on what the regions below took, so it is not
/// here. This is everything that is known before any of that.
pub fn description_tree_size(code: RegionCode) -> usize {
    let mut size = CODE_WIDTH + if code.is_masked() { CODE_WIDTH + CHILD_MASK_WIDTH } else { 0 };
    match code {
        RegionCode::Bind { level, .. } => size += tile_size_field_width(level),
        RegionCode::Subdivide { .. } => {}
        RegionCode::Copy { .. } => size += DIRECTION_WIDTH,
    }
    size
}

/// What a description costs all told: what it spends on itself, what
/// every region it describes again will spend, and whatever payload
/// the binding above is left writing for the parts of it nobody took.
///
/// The three are one number because they are one decision. A region
/// that takes the whole of itself leaves the binding above nothing to
/// write over its area; a region that puts right a corner of itself
/// leaves the binding writing all the rest. So the bits a binding
/// spends inside a region are counted here, where whether they exist
/// is settled, and not up at the binding, which cannot know.
pub fn whole_subtree_size(
    work: &Workspace,
    region: Region,
    code: RegionCode,
    standing: Standing,
) -> usize {
    let mut size = description_tree_size(code);
    // A binding at one tile keeps one bit, unless something below
    // takes the whole region out from under it. A region that only
    // puts part of itself right leaves the tile it stands in still
    // needing its bit.
    if matches!(code, RegionCode::Bind { depth: 0, .. })
        || (matches!(code, RegionCode::Subdivide { .. }) && standing == Standing::Tiles(0))
    {
        size += 1;
    }
    let mask = code.mask();
    let theirs = standing_under(work, standing, region, code);
    for (child, at) in region.children().into_iter().zip(0..CHILD_COUNT) {
        size += cost_of_a_child(work, child, theirs, mask.describes(at));
    }
    size
}

/// What one child comes to, described or left where it is.
///
/// A child left to a binding's tiles costs those tiles' bits and
/// nothing else. A child left to one thing being said costs nothing
/// at all: the bit that says it was going to be written anyway.
fn cost_of_a_child(work: &Workspace, child: Region, standing: Standing, described: bool) -> usize {
    match (standing, described) {
        (Standing::Tiles(depth), true) => work.cost_under_tiles_of(child, depth),
        (Standing::Tiles(depth), false) => tiles_at_depth(depth),
        (_, false) => 0,
        (Standing::Nothing, true) => work.cost_of(child),
        (_, true) => cost_of_describing(work, child, standing),
    }
}

/// What a 4x4 spends when it always masks: the mask, and per child
/// either a direction to copy from or its four cells.
///
/// There is no code. The region is bound by definition, so the only
/// thing left to say is which children are copied, and from where.
pub fn four_by_four_mask_size(children_copied: usize) -> usize {
    CHILD_MASK_WIDTH
        + children_copied * DIRECTION_WIDTH
        + (CHILD_COUNT - children_copied) * CELLS_IN_A_CHILD
}

/// Cells in a child of a 4x4, which is the four of a 2x2.
pub const CELLS_IN_A_CHILD: usize = 4;

/// What a region costs when nothing is said about it at all: its cells
/// written out, one bit each.
pub fn cells_written_out(region: Region) -> usize {
    tiles_at_depth(deepest_depth(region.level))
}

/// What it costs to describe a region, standing in what it stands in.
///
/// A region with a binding above saying one thing over it only has to
/// put right what that gets wrong. A region that is already right has
/// nothing to put right -- but a mask that names it has asked it to
/// say something anyway, and the least it can say is the whole of
/// itself.
pub fn cost_of_describing(work: &Workspace, region: Region, standing: Standing) -> usize {
    match standing.reads() {
        Some(reads) => match work.cost_to_put_right_of(region, reads) {
            0 => work.cost_of(region),
            putting_right => putting_right,
        },
        None => work.cost_of(region),
    }
}

/// Whether a region is too fine to have a grammar of its own.
pub fn below_the_grammar(region: Region) -> bool {
    region.level > FINEST_LEVEL_WITH_A_GRAMMAR
}

/// Whether a region has children at all.
pub fn has_children(region: Region) -> bool {
    region.level < CELL_LEVEL
}
