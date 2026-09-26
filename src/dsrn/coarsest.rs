//! The pass that runs before anything is written: what tile size each
//! region could be bound at, and what its cheapest description costs.
//!
//! It runs cells upward, so a region is priced only once its children
//! are. Two numbers come out of it per region and both are wanted by
//! the region above: the coarsest tile size that tiles it
//! homogeneously, which says whether a parent could bind over it, and
//! what describing it costs, which says whether a parent should.
//!
//! The price of a copy is what it would cost if the neighbour were
//! already encoded. Whether it will be depends on what every region
//! above chooses, which happens after this, so this cannot know -- and
//! does not have to. Pricing a copy at four bits only ever makes a
//! region look cheaper than it turns out to be, and the descent asks
//! the real question before it writes anything.

use crate::dsrn::cost::{below_the_grammar, cells_written_out, whole_subtree_size};
use crate::dsrn::describable::every_description;
use crate::dsrn::nesting::Masking;
use crate::dsrn::nesting_data::Workspace;
use crate::dsrn::region::Region;
use crate::pyramid::{tile_of_bitmap, Pyramid, CELL_LEVEL};
use crate::Bitmap;

/// Fills the workspace for a region and everything under it.
pub fn coarsest(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    masking: Masking,
) {
    if region.is_a_cell() {
        work.set_coarsest_depth(region, 0);
        work.set_cost(region, 1);
        return;
    }

    let all_one_thing =
        tile_of_bitmap(pyramid, bitmap, region.level, region.x, region.y).is_some();
    let mut deepest = 0;
    for child in region.children() {
        coarsest(work, pyramid, bitmap, child, masking);
        deepest = deepest.max(work.coarsest_depth_of(child));
    }
    // One level finer than its deepest child, unless it is all one
    // thing and is a single tile. A region below the grammar still
    // needs this, so that a parent can ask whether it could bind over
    // it.
    work.set_coarsest_depth(region, if all_one_thing { 0 } else { deepest + 1 });

    if below_the_grammar(region) {
        work.set_cost(region, cells_written_out(region));
        return;
    }

    let cheapest = every_description(work, pyramid, bitmap, region, masking, false)
        .into_iter()
        .map(|code| whole_subtree_size(work, region, code))
        .min()
        .expect("every region can at least bind at one cell a tile");
    work.set_cost(region, cheapest);
    let _ = CELL_LEVEL;
}
