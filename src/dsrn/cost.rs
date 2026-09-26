//! What a description costs, in bits.
//!
//! Two numbers, and keeping them apart is what lets an encode be
//! checked against itself. [`bound_region_payload_size`] and
//! [`description_size`] are what a region spends on itself; adding
//! that up over every region described must come to exactly what the
//! encode wrote. [`whole_subtree_size`] is that plus everything the
//! description hands on, which is what a region is worth to the region
//! above it.

use crate::dsrn::nesting_data::{
    RegionCode, RegionMask, Workspace, CHILD_MASK_WIDTH, CODE_WIDTH, DIRECTION_WIDTH,
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

/// How many payload bits a binding writes.
///
/// This is the whole of the answer to "does a binding write bits for
/// tiles inside a region it handed on": it does not. A binding covers
/// its whole region, so it writes a bit for every one of its tiles --
/// except the ones that fall inside a child it described again, which
/// took those cells for itself and pays for them.
pub fn bound_region_payload_size(depth: usize, mask: RegionMask) -> usize {
    if mask == RegionMask::NONE {
        tiles_at_depth(depth)
    } else {
        mask.left_to_a_binding() * tiles_at_depth(depth - 1)
    }
}

/// What a description spends on itself: everything but the regions it
/// hands on.
pub fn description_size(code: RegionCode) -> usize {
    let mut size = CODE_WIDTH + if code.is_masked() { CODE_WIDTH + CHILD_MASK_WIDTH } else { 0 };
    match code {
        RegionCode::Bind { level, depth, mask } => {
            size += tile_size_field_width(level);
            size += bound_region_payload_size(depth, mask);
        }
        RegionCode::Subdivide { .. } => {}
        RegionCode::Copy { .. } => size += DIRECTION_WIDTH,
    }
    size
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

/// What a description costs all told: what it spends on itself, and
/// what every region it hands on will spend, down to the cells.
///
/// The handed on part is read from the workspace, which holds each
/// region's own whole subtree size, so this is the whole subtree and
/// not just the children.
pub fn whole_subtree_size(work: &Workspace, region: Region, code: RegionCode) -> usize {
    let mut size = description_size(code);
    let mask = code.mask();
    for (child, at) in region.children().into_iter().zip(0..CHILD_COUNT) {
        if mask.describes(at) {
            size += work.cost_of(child);
        }
    }
    size
}

/// Whether a region is too fine to have a grammar of its own.
pub fn below_the_grammar(region: Region) -> bool {
    region.level > FINEST_LEVEL_WITH_A_GRAMMAR
}

/// Whether a region has children at all.
pub fn has_children(region: Region) -> bool {
    region.level < CELL_LEVEL
}
