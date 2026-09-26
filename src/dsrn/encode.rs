//! The descent that writes the encoding.
//!
//! It runs the whole bitmap downward, and at every region it asks the
//! same three things in the same order: what could this region say,
//! what would each of those cost, and which is smallest. Then it
//! writes that and moves on to whatever the description left out.
//!
//! The one thing it knows that [`super::coarsest`] could not is which
//! cells are encoded already, so a copy offered here is a copy the
//! decoder will really be able to make.

use crate::dsrn::cost::{
    below_the_grammar, description_size, tile_size_field_width, whole_subtree_size,
};
use crate::dsrn::describable::every_description;
use crate::dsrn::nesting::Masking;
use crate::dsrn::nesting_data::{
    Encoded, RegionCode, Workspace, BIND, CHILD_MASK_WIDTH, CODE_WIDTH, COPY, DIRECTION_WIDTH,
    MASK, SUBDIVIDE,
};
use crate::dsrn::region::{deepest_depth, tiles_at_depth, Region, CHILD_COUNT, DIRECTIONS};
use crate::pyramid::{tile_of_bitmap, Pyramid};
use crate::Bitmap;

/// Describes one region, and whatever its description leaves out.
pub fn encode_region(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    masking: Masking,
    out: &mut Encoded,
) {
    if below_the_grammar(region) {
        write_the_cells(work, pyramid, bitmap, region, out);
        return;
    }

    let code = every_description(work, pyramid, bitmap, region, masking, true)
        .into_iter()
        .min_by_key(|&code| whole_subtree_size(work, region, code))
        .expect("every region can at least bind at one cell a tile");

    out.counts.accounted += description_size(code);
    if code.is_masked() {
        out.tree.push_value(MASK, CODE_WIDTH);
    }
    match code {
        RegionCode::Bind { depth, mask, .. } => {
            count_a_binding(bitmap, region, depth, out);
            out.tree.push_value(BIND, CODE_WIDTH);
            if code.is_masked() {
                out.counts.masked_bindings += 1;
                out.tree.push_value(mask.0, CHILD_MASK_WIDTH);
            }
            out.tree.push_value(depth as u64, tile_size_field_width(region.level));
            // A payload bit for every tile of every child it keeps, in
            // reading order.
            for tile in region.tiles_at_depth(depth) {
                if mask != crate::dsrn::nesting_data::RegionMask::EVERY
                    && !mask.covers(region.child_holding(depth, tile))
                {
                    continue;
                }
                let value = tile_of_bitmap(pyramid, bitmap, tile.level, tile.x, tile.y)
                    .expect("a bound tile is homogeneous, or the binding would be a lie");
                out.payload.push(value);
                if tile.is_a_cell() {
                    out.counts.cells_written += 1;
                }
                work.mark_encoded(tile);
            }
        }
        RegionCode::Subdivide { mask } => {
            out.counts.subdivides += 1;
            out.tree.push_value(SUBDIVIDE, CODE_WIDTH);
            if code.is_masked() {
                out.counts.masked_subdivides += 1;
                out.counts.children_left_clear += mask.covered();
                out.tree.push_value(mask.0, CHILD_MASK_WIDTH);
            }
            // A child the mask covers is left clear, which the decoder
            // already holds it as.
            for (at, child) in region.children().into_iter().enumerate() {
                if mask.covers(at) {
                    work.mark_encoded(child);
                }
            }
        }
        RegionCode::Copy { direction, mask } => {
            out.counts.copies += 1;
            out.tree.push_value(COPY, CODE_WIDTH);
            if code.is_masked() {
                out.counts.masked_copies += 1;
                out.tree.push_value(mask.0, CHILD_MASK_WIDTH);
            }
            out.tree.push_value(direction as u64, DIRECTION_WIDTH);
            if mask == crate::dsrn::nesting_data::RegionMask::EVERY {
                work.mark_encoded(region);
            } else {
                for (at, child) in region.children().into_iter().enumerate() {
                    if mask.covers(at) {
                        work.mark_encoded(child);
                    }
                }
            }
        }
    }

    // Whatever the description left out is described before the
    // descent moves on, so a region beside it may read what it wrote.
    let mask = code.mask();
    for (at, child) in region.children().into_iter().enumerate() {
        if !mask.covers(at) {
            out.counts.children_made_regions += 1;
            encode_region(work, pyramid, bitmap, child, masking, out);
        }
    }
    let _ = CHILD_COUNT;
}

/// A region below the grammar: its cells, in reading order, and no
/// code to say that is what they are.
fn write_the_cells(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    out: &mut Encoded,
) {
    let depth = deepest_depth(region.level);
    out.counts.below_the_grammar += 1;
    out.counts.accounted += tiles_at_depth(depth);
    for cell in region.tiles_at_depth(depth) {
        let value = tile_of_bitmap(pyramid, bitmap, cell.level, cell.x, cell.y)
            .expect("a cell is all one thing");
        out.payload.push(value);
        out.counts.cells_written += 1;
    }
    work.mark_encoded(region);
}

/// The counts that are about a binding giving up rather than about the
/// code it wrote.
fn count_a_binding(bitmap: &Bitmap, region: Region, depth: usize, out: &mut Encoded) {
    out.counts.bindings += 1;
    if depth != deepest_depth(region.level) {
        return;
    }
    out.counts.bound_at_cells += 1;
    out.counts.cells_given_up[region.level] += tiles_at_depth(depth);
    // Whether a copy was there to be had and reading order took it
    // away, or there was never one.
    let matched = (0..DIRECTIONS.len()).any(|direction| {
        region
            .neighbour(direction)
            .is_some_and(|from| crate::dsrn::region::same_cells(bitmap, region, from))
    });
    if matched {
        out.counts.copies_just_missed += 1;
    } else {
        out.counts.no_neighbour_matched += 1;
    }
}
