//! The descent that writes the encoding.
//!
//! It runs the whole bitmap downward, and at every region it asks the
//! same three things in the same order: what could this region say,
//! what would each of those cost, and which is smallest. Then it
//! writes that and moves on.
//!
//! What it writes is in two halves and they go in opposite orders. A
//! region's own fields go into the tree before its children's, so the
//! tree reads top down. Its own cells go into the payload after its
//! children's, because a binding writes nothing for a tile a region
//! below it has already taken, and it only knows what they took once
//! they have been. The two streams are separate, so both orders hold
//! at once.
//!
//! The one thing it knows that [`super::coarsest`] could not is which
//! cells are taken already, so a copy offered here is a copy the
//! decoder will really be able to make.

use crate::dsrn::cost::{
    below_the_grammar, description_tree_size, four_by_four_mask_size, tile_size_field_width,
    whole_subtree_size,
};
use crate::dsrn::describable::{
    every_description, standing_under, where_each_child_copies_from,
};
use crate::dsrn::four_by_four::{
    what_a_four_by_four_says, TheCode, WhatItLeaves, BIND, COPY, COPY_OR_LEAVE_WIDTH, FIRST_WIDTH,
    MASKED, MASKED_OR_NOT_WIDTH, NOT_BIND, SECOND_WIDTH, SKIP, THE_REST_ARE_LEFT, THE_REST_COPY,
    TILES_OF_FOUR, TILES_OF_ONE, TILE_SIZE_WIDTH, UNMASKED,
};
use crate::dsrn::nesting::Knobs;
use crate::dsrn::nesting_data::{
    Encoded, RegionCode, RegionMask, Standing, Workspace, A_SIZE_THAT_MEANS_COPY,
    BIND as BIND_CODE, CHILD_MASK_WIDTH, CODE_WIDTH, COPY as COPY_CODE, DIRECTION_WIDTH, MASK,
    SUBDIVIDE,
};
use crate::dsrn::region::{
    deepest_depth, same_cells, tiles_at_depth, Region, CHILD_COUNT,
    DIRECTIONS,
};
use crate::pyramid::{tile_of_bitmap, Pyramid};
use crate::Bitmap;

/// Describes one region, the children it describes again, and then
/// whatever of itself they left.
///
/// `standing` is what the closest binding above already says over
/// this region, which is what a child left out of a mask is left to.
/// At the top of the bitmap there is no such binding, and what is
/// left out stays clear.
pub fn encode_region(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    knobs: Knobs,
    standing: Standing,
    out: &mut Encoded,
) {
    if below_the_grammar(region) {
        write_the_cells(work, pyramid, bitmap, region, out);
        return;
    }

    if knobs.four_by_four.applies_to(region) {
        write_a_mask_over_the_children(work, pyramid, bitmap, region, out);
        return;
    }

    if knobs.four_by_four.is_its_own_grammar(region) {
        write_what_a_four_by_four_says(work, pyramid, bitmap, region, knobs, standing, out);
        return;
    }

    let code = every_description(work, pyramid, bitmap, region, knobs, true, standing)
        .into_iter()
        .min_by_key(|&code| whole_subtree_size(work, region, code, standing))
        .expect("every region can at least bind at one cell a tile");

    let mask = code.mask();
    // The tree half now; the payload half is counted as it goes out,
    // because how much of it there is depends on what the regions
    // below take.
    out.counts.accounted += description_tree_size(code);
    if code.is_masked() {
        out.tree.push_value(MASK, CODE_WIDTH);
        out.counts.children_left_to_a_binding += mask.left_to_a_binding();
    }
    match code {
        RegionCode::Bind { depth, .. } => {
            count_a_binding(pyramid, region, depth, out);
            out.tree.push_value(BIND_CODE, CODE_WIDTH);
            if code.is_masked() {
                out.counts.masked_bindings += 1;
                out.tree.push_value(mask.0, CHILD_MASK_WIDTH);
            }
            out.tree.push_value(depth as u64, tile_size_field_width(region.level));
        }
        RegionCode::Subdivide { .. } => {
            out.counts.subdivides += 1;
            out.tree.push_value(SUBDIVIDE, CODE_WIDTH);
            if code.is_masked() {
                out.counts.masked_subdivides += 1;
                out.tree.push_value(mask.0, CHILD_MASK_WIDTH);
            }
        }
        RegionCode::Copy { direction, .. } => {
            out.counts.copies += 1;
            out.tree.push_value(COPY_CODE, CODE_WIDTH);
            if code.is_masked() {
                out.counts.masked_copies += 1;
                out.tree.push_value(mask.0, CHILD_MASK_WIDTH);
            }
            out.tree.push_value(direction as u64, DIRECTION_WIDTH);
        }
        // A bind whose tile size field takes the one value it has no
        // size for, then its own mask, then where each child it names
        // copies from.
        RegionCode::CopyEachChild { .. } => {
            out.counts.four_by_fours_copying_each_child += 1;
            out.counts.children_copying_themselves += mask.described();
            out.counts.children_left_to_a_binding += mask.left_to_a_binding();
            out.tree.push_value(BIND_CODE, CODE_WIDTH);
            out.tree
                .push_value(A_SIZE_THAT_MEANS_COPY, tile_size_field_width(region.level));
            out.tree.push_value(mask.0, CHILD_MASK_WIDTH);
            let from = where_each_child_copies_from(work, pyramid, bitmap, region, true);
            for (at, child) in region.children().into_iter().enumerate() {
                if !mask.describes(at) {
                    continue;
                }
                let direction = from[at].expect("a child this names was offered a direction");
                out.tree.push_value(direction as u64, DIRECTION_WIDTH);
                out.counts.copied_tiles_at_level[child.level] += 1;
                work.mark_encoded(child);
            }
        }
    }

    // The children described again go first, because what they take
    // is exactly what this region does not have to write.
    let theirs = standing_under(work, standing, region, code);
    let says_it_all = matches!(code, RegionCode::CopyEachChild { .. });
    for (at, child) in region.children().into_iter().enumerate() {
        if mask.describes(at) && !says_it_all {
            out.counts.children_made_regions += 1;
            encode_region(work, pyramid, bitmap, child, knobs, theirs, out);
        }
    }

    match code {
        RegionCode::Bind { depth, .. } => {
            // A payload bit for every tile of the region, in reading
            // order, but for the ones a region below took whole.
            for tile in region.tiles_at_depth(depth) {
                let Some(value) = what_is_left_of(work, bitmap, tile) else { continue };
                out.payload.push(value);
                out.counts.accounted += 1;
                out.counts.bound_tiles_at_level[tile.level] += 1;
                if tile.is_a_cell() {
                    out.counts.cells_written += 1;
                }
                work.mark_encoded(tile);
            }
        }
        // A copy takes what is left of the region from the neighbour.
        RegionCode::Copy { .. } => {
            out.counts.copied_tiles_at_level[region.level] += 1;
            work.mark_encoded(region);
        }
        // A subdivision writes nothing. A child it left out is the
        // binding above's to fill; where there is none, it stays
        // clear, and clear is something the decoder holds and a copy
        // may read.
        RegionCode::Subdivide { .. } | RegionCode::CopyEachChild { .. } => {
            if standing == Standing::Clear {
                for (at, child) in region.children().into_iter().enumerate() {
                    if !mask.describes(at) {
                        work.mark_encoded(child);
                    }
                }
            }
        }
    }
    let _ = CHILD_COUNT;
}

/// What a binding has left to say about one of its tiles: the value
/// of the cells no region below took, or nothing at all if they took
/// the tile whole.
///
/// Those cells have to be one thing, because one bit is all a binding
/// has to say about them. They are not the whole tile: a tile with an
/// override inside it is one thing only around the override.
fn what_is_left_of(work: &Workspace, bitmap: &Bitmap, tile: Region) -> Option<bool> {
    let (x, y) = tile.top_left_cell();
    let side = tile.side_in_cells();
    let mut left = None;
    for row in 0..side {
        for col in 0..side {
            let (at_x, at_y) = ((x + col) as u8, (y + row) as u8);
            if work.encoded_cells.get(at_x, at_y) {
                continue;
            }
            let value = bitmap.get(at_x, at_y);
            match left {
                None => left = Some(value),
                Some(so_far) => assert_eq!(
                    so_far, value,
                    "a binding has one bit for what is left of a tile, and it is not one thing"
                ),
            }
        }
    }
    left
}

/// A 4x4 in its own grammar: a bit for the code, a bit for the tile
/// size where it binds, a bit for whether it masks, and then the mask
/// and a bit for what becomes of what it leaves.
///
/// Children in reading order after that: what the code takes it says
/// outright, and what it leaves is a direction or nothing at all.
fn write_what_a_four_by_four_says(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    knobs: Knobs,
    standing: Standing,
    out: &mut Encoded,
) {
    let (says, _) = what_a_four_by_four_says(work, pyramid, bitmap, region, knobs, true, standing);
    let from = where_each_child_copies_from(work, pyramid, bitmap, region, true);
    out.counts.four_by_fours_in_their_own_grammar += 1;
    let already_right = crate::dsrn::describable::children_standing_gets_wrong(
        work, pyramid, bitmap, region, standing,
    ) == RegionMask::NONE;
    let spent_so_far = out.counts.accounted;
    if already_right {
        out.counts.four_by_fours_already_right += 1;
    }
    out.counts.accounted += says.header_size();

    match says.code {
        TheCode::BindAtFours | TheCode::BindAtOnes => {
            out.tree.push_value(BIND, FIRST_WIDTH);
            let size =
                if says.code == TheCode::BindAtOnes { TILES_OF_ONE } else { TILES_OF_FOUR };
            out.tree.push_value(size, TILE_SIZE_WIDTH);
        }
        TheCode::Skip => {
            out.tree.push_value(NOT_BIND, FIRST_WIDTH);
            out.tree.push_value(SKIP, SECOND_WIDTH);
            if says.mask.is_none() {
                out.counts.four_by_fours_left_whole += 1;
            }
        }
        TheCode::Copied { direction } => {
            out.counts.copies += 1;
            out.counts.four_by_fours_copied_whole += 1;
            out.counts.copied_tiles_at_level[region.level] += 1;
            out.tree.push_value(NOT_BIND, FIRST_WIDTH);
            out.tree.push_value(COPY, SECOND_WIDTH);
            out.tree.push_value(direction as u64, DIRECTION_WIDTH);
        }
    }
    match says.mask {
        None => out.tree.push_value(UNMASKED, MASKED_OR_NOT_WIDTH),
        Some((mask, leaves)) => {
            out.tree.push_value(MASKED, MASKED_OR_NOT_WIDTH);
            out.tree.push_value(mask.0, CHILD_MASK_WIDTH);
            let rest =
                if leaves == WhatItLeaves::Copies { THE_REST_COPY } else { THE_REST_ARE_LEFT };
            out.tree.push_value(rest, COPY_OR_LEAVE_WIDTH);
        }
    }

    for (at, child) in region.children().into_iter().enumerate() {
        if !says.takes_it(at) {
            match says.mask {
                Some((_, WhatItLeaves::Copies)) => {
                    let direction = from[at].expect("a child left to copy was offered a direction");
                    out.tree.push_value(direction as u64, DIRECTION_WIDTH);
                    out.counts.accounted += DIRECTION_WIDTH;
                    out.counts.children_copying_themselves += 1;
                    out.counts.copied_tiles_at_level[child.level] += 1;
                    work.mark_encoded(child);
                }
                // Left to the binding above, which writes it. Where
                // there is no binding above it stays clear, and clear
                // is something the decoder holds.
                _ => {
                    out.counts.children_left_to_a_binding += 1;
                    if standing == Standing::Clear {
                        work.mark_encoded(child);
                    }
                }
            }
            continue;
        }
        match says.code {
            TheCode::BindAtFours => {
                let value = tile_of_bitmap(pyramid, bitmap, child.level, child.x, child.y)
                    .expect("a child bound at one tile is all one thing");
                out.payload.push(value);
                out.counts.accounted += 1;
                out.counts.bound_tiles_at_level[child.level] += 1;
                work.mark_encoded(child);
            }
            TheCode::BindAtOnes | TheCode::Skip => {
                write_the_cells(work, pyramid, bitmap, child, out);
            }
            // The cells come from the neighbour the code named, and
            // the decoder reads them there. Nothing is written for it
            // beyond the direction already in the header.
            TheCode::Copied { .. } => work.mark_encoded(child),
        }
    }
    if already_right {
        out.counts.bits_spent_on_being_already_right += out.counts.accounted - spent_so_far;
    }
    let which = match says.code {
        TheCode::BindAtFours => 0,
        TheCode::BindAtOnes => 1,
        TheCode::Skip => 2,
        TheCode::Copied { .. } => 3,
    };
    out.counts.four_by_four_said[which] += 1;
    out.counts.four_by_four_spent[which] += out.counts.accounted - spent_so_far;
}

/// A 4x4 that always masks: a four bit mask, then per child in
/// reading order either a direction to copy from or its four cells.
///
/// No code. The region is bound by definition, and the mask is the
/// only thing left to say about it. A child is written before the
/// next is looked at, so a child may copy from the one beside it.
fn write_a_mask_over_the_children(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    out: &mut Encoded,
) {
    // Child by child, in reading order, because a child may copy from
    // the one beside it and so may only be asked once that one is
    // written.
    let (mut mask, mut from) = (0u64, [None; CHILD_COUNT]);
    for (at, child) in region.children().into_iter().enumerate() {
        if pyramid.copyable(child.level, child.x, child.y) {
            from[at] = (0..DIRECTIONS.len()).find(|&direction| {
                child.neighbour(direction).is_some_and(|beside| {
                    same_cells(bitmap, child, beside)
                        && work.region_taken.whole_region_taken(beside)
                })
            });
        }
        if from[at].is_some() {
            mask |= 1 << at;
        }
        work.mark_encoded(child);
    }

    let copied = mask.count_ones() as usize;
    out.counts.four_by_four_masks += 1;
    out.counts.children_copied += copied;
    out.counts.accounted += four_by_four_mask_size(copied);
    out.tree.push_value(mask, CHILD_MASK_WIDTH);
    for (at, child) in region.children().into_iter().enumerate() {
        match from[at] {
            Some(direction) => {
                out.tree.push_value(direction as u64, DIRECTION_WIDTH);
                out.counts.copied_tiles_at_level[child.level] += 1;
            }
            None => {
                for cell in child.tiles_at_depth(deepest_depth(child.level)) {
                    let value = tile_of_bitmap(pyramid, bitmap, cell.level, cell.x, cell.y)
                        .expect("a cell is all one thing");
                    out.payload.push(value);
                    out.counts.cells_written += 1;
                    out.counts.bound_tiles_at_level[cell.level] += 1;
                }
            }
        }
    }
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
        out.counts.bound_tiles_at_level[cell.level] += 1;
    }
    work.mark_encoded(region);
}

/// The counts that are about a binding giving up rather than about the
/// code it wrote.
fn count_a_binding(pyramid: &Pyramid, region: Region, depth: usize, out: &mut Encoded) {
    out.counts.bindings += 1;
    if depth != deepest_depth(region.level) {
        return;
    }
    out.counts.bound_at_cells += 1;
    out.counts.cells_given_up[region.level] += tiles_at_depth(depth);
    // Whether a copy was there to be had and reading order took it
    // away, or there was never one, which the pyramid has already
    // worked out.
    if pyramid.copyable(region.level, region.x, region.y) {
        out.counts.copies_just_missed += 1;
    } else {
        out.counts.no_neighbour_matched += 1;
    }
}
