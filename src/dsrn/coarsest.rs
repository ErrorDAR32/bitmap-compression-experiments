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
//! the real question before it writes anything. Whether a binding
//! above covers this region is the same kind of unknown, priced the
//! same way: as though nothing did.

use crate::dsrn::cost::{
    below_the_grammar, cells_written_out, four_by_four_mask_size, whole_subtree_size,
};
use crate::dsrn::describable::{every_description, where_each_child_copies_from};
use crate::dsrn::nesting::Knobs;
use crate::dsrn::nesting_data::{Standing, Workspace};
use crate::dsrn::region::{deepest_depth, Region};
use crate::pyramid::{tile_of_bitmap, Pyramid, CELL_LEVEL};
use crate::Bitmap;

/// Fills the workspace for a region and everything under it.
pub fn coarsest(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    knobs: Knobs,
) {
    if region.is_a_cell() {
        work.set_coarsest_depth(region, 0);
        work.set_cost(region, 1);
        price_putting_right(work, pyramid, bitmap, region, 1);
        return;
    }

    let all_one_thing =
        tile_of_bitmap(pyramid, bitmap, region.level, region.x, region.y).is_some();
    let mut deepest = 0;
    for child in region.children() {
        coarsest(work, pyramid, bitmap, child, knobs);
        deepest = deepest.max(work.coarsest_depth_of(child));
    }
    // One level finer than its deepest child, unless it is all one
    // thing and is a single tile. A region below the grammar still
    // needs this, so that a parent can ask whether it could bind over
    // it.
    work.set_coarsest_depth(region, if all_one_thing { 0 } else { deepest + 1 });

    if below_the_grammar(region) {
        work.set_cost(region, cells_written_out(region));
        price_putting_right(work, pyramid, bitmap, region, cells_written_out(region));
        return;
    }

    // A 4x4 that always masks has nothing to choose between. It costs
    // what its children cost, and they are priced on whether their
    // cells match a neighbour at all -- the descent asks whether the
    // decoder will hold that neighbour.
    if knobs.four_by_four.applies_to(region) {
        let copied = where_each_child_copies_from(work, bitmap, region, false)
            .iter()
            .filter(|from| from.is_some())
            .count();
        work.set_cost(region, four_by_four_mask_size(copied));
        price_putting_right(work, pyramid, bitmap, region, four_by_four_mask_size(copied));
        return;
    }

    // Three times over, because what a region has to say depends on
    // what is already standing over it, and there are three kinds of
    // that: nothing, one thing said, and a binding's tiles. They go
    // in this order because each is asked in terms of the last.
    work.set_cost(region, cheapest_in(work, pyramid, bitmap, region, knobs, Standing::Nothing));
    for standing in [false, true] {
        let cost = if already_reads(pyramid, bitmap, region, standing) {
            0
        } else {
            cheapest_in(work, pyramid, bitmap, region, knobs, Standing::Reads(standing))
        };
        work.set_cost_to_put_right(region, standing, cost);
    }
    for depth in 0..=deepest_depth(region.level) {
        let cost = cheapest_in(work, pyramid, bitmap, region, knobs, Standing::Tiles(depth));
        work.set_cost_under_tiles(region, depth, cost);
    }
    let _ = CELL_LEVEL;
}

/// The cheapest thing a region could say, standing in what it stands
/// in.
fn cheapest_in(
    work: &Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    knobs: Knobs,
    standing: Standing,
) -> usize {
    every_description(work, pyramid, bitmap, region, knobs, false, standing)
        .into_iter()
        .map(|code| whole_subtree_size(work, region, code, standing))
        .min()
        .expect("every region can at least bind at one cell a tile")
}

/// Whether a region already reads what a binding above would say
/// over it, and so has nothing to put right and nothing to say.
fn already_reads(pyramid: &Pyramid, bitmap: &Bitmap, region: Region, standing: bool) -> bool {
    tile_of_bitmap(pyramid, bitmap, region.level, region.x, region.y) == Some(standing)
}

/// What a region with nothing to choose between costs, whatever is
/// standing over it.
///
/// It has one description and no way to say part of itself, so a
/// binding above changes only whether it is described at all: nothing
/// where it already reads what is standing, and the whole of its one
/// description where it does not. Under a binding's tiles it is
/// always the whole, because the choice of saying only part of itself
/// is the one it does not have.
fn price_putting_right(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    cost: usize,
) {
    for standing in [false, true] {
        let right = already_reads(pyramid, bitmap, region, standing);
        work.set_cost_to_put_right(region, standing, if right { 0 } else { cost });
    }
    for depth in 0..=deepest_depth(region.level) {
        work.set_cost_under_tiles(region, depth, cost);
    }
}
