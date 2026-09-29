//! The greedy tiler, the first pass: one rule, asked of the whole bitmap,
//! then of every tile nothing coarser says, down to 2x2s:
//!
//! 1. Homogeneous? Bind it.
//! 2. Down to 4x4, copyable (a same-size neighbour, or, one level up, a
//!    same-size neighbour of the tile's own parent)? Copy it.
//! 3. Down to 8x8, does a copy say at least [`MIN_UNMASKED_CHILDREN`] of
//!    its children, or at least [`MIN_UNMASKED_NON_HOMOGENEOUS_CHILDREN`]
//!    that are not homogeneous? Copy it, masking the others.
//! 4. Down to 8x8, are at least [`MIN_UNMASKED_CHILDREN_OF_A_MASKING_BIND`]
//!    children homogeneous with the value not bound above? Bind it to
//!    that value, masking the others: every child it leaves unnamed, at
//!    any depth, is bound by it. Clear is bound at the top.
//! 5. Else leave it for its four children to try for themselves.
//!
//! Masked children are left to the tiles placed inside them. No
//! comparison ever happens between sizes; a tile that qualifies is
//! taken immediately. What a tile gets depends only on its own cells
//! and on what its ancestors got, so the pass walks down depth first,
//! carrying the value bound above, into the children of a tile left
//! unplaced and the children a placed tile masks. Cells are always
//! homogeneous, so the pass always covers the whole bitmap. Which binds
//! are left to the binding above is the tree's to say, not this pass's:
//! a bind stays a bind here, for the complex tiler to unmask if that is
//! cheaper.
//!
//! A 2x2 is only ever asked whether it is homogeneous: if not, nothing
//! is placed in it -- its cells are said in the last pass or by a
//! complex tile of 1x1 resolution -- so nothing finer than a 2x2 is ever
//! placed. The tree goes no finer than 4x4: a 2x2 placed is read only
//! by a complex tile of 2x2 resolution that unmasks it.

use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::pyramids::copyable::{child_offset, matches_at, matching_direction, CopyOffsets, FINEST_COPY_LEVEL};
use crate::gct::pyramids::placements::{Placement, BOUND_AT_THE_TOP, FINEST_MASKING_LEVEL};
use crate::gct::pyramids::patterns::Patterns;
use crate::gct::tile::{directions, Tile, FINEST_PLACED_LEVEL};
use crate::Bitmap;

/// A masking copy costs about 10 bits before its masked children: a
/// copy, a mask-present bit, a 4-bit child mask. What it saves depends
/// on what the children it says would cost without it. A homogeneous
/// one is cheap anyway -- a tile, or 1 bit unmasked in a complex tile,
/// which masking it away also takes from the complex tiler. Any other
/// needs a copy or a subtree of its own, 5 bits or more. So a masking
/// copy must say at least this many children...
pub const MIN_UNMASKED_CHILDREN: u32 = 3;
/// ...or at least this many that are not homogeneous. Either half alone
/// measured worse (`docs/gct.md`).
pub const MIN_UNMASKED_NON_HOMOGENEOUS_CHILDREN: u32 = 2;

/// A masking bind, spelled as a divide that flips the value bound above,
/// costs 7 bits before its masked children; each child it says would
/// otherwise be a tile of its own, 4 bits or more. So it must say at
/// least this many children.
pub const MIN_UNMASKED_CHILDREN_OF_A_MASKING_BIND: u32 = 2;

/// Places tiles over one bitmap, biggest first, into `placements`,
/// whatever it held before; what it placed is a
/// [complex tiling pyramid](crate::gct::pyramids::complex_tiling) with
/// only its [placement](crate::gct::pyramids::placements) bits set. Reads
/// only the bitmap's content: which tiles are homogeneous and which
/// match which, from its patterns -- copies reading from `offsets` -- and
/// a 2x2's cells, finer than patterns go.
pub fn greedy_tiler(bitmap: &Bitmap, patterns: &Patterns, offsets: &CopyOffsets, placements: &mut ComplexTiling) {
    placements.clear();
    let content = Content { bitmap, patterns, offsets };
    place_at_or_under(&content, Tile::whole_bitmap(), BOUND_AT_THE_TOP, placements);
}

/// What the greedy tiler reads of a bitmap: its cells and its patterns
/// -- and where copies read from.
struct Content<'a> {
    /// Its cells.
    bitmap: &'a Bitmap,
    /// Its patterns.
    patterns: &'a Patterns,
    /// Where copies read from.
    offsets: &'a CopyOffsets,
}

/// Places `tile`, or leaves it to its children, then does the same for
/// every child nothing placed here says; `bound_above` the value bound
/// above `tile`.
fn place_at_or_under(content: &Content, tile: Tile, bound_above: bool, placements: &mut ComplexTiling) {
    let Some(placement) = placement(content, tile, bound_above) else {
        if tile.level == FINEST_PLACED_LEVEL {
            // A 2x2 that is not one tile: nothing reads a placement finer
            // than a 2x2.
            return;
        }
        for child in tile.children() {
            place_at_or_under(content, child, bound_above, placements);
        }
        return;
    };
    placements.place(tile, placement);
    let bound_inside = placement.bound_inside(bound_above);
    for child in tile.children() {
        if placement.masks(child) {
            place_at_or_under(content, child, bound_inside, placements);
        }
    }
}

/// What the rule places at `tile`, if anything.
fn placement(content: &Content, tile: Tile, bound_above: bool) -> Option<Placement> {
    if let Some(value) = homogeneous_value(content, tile) {
        return Some(Placement::bound(value));
    }
    if tile.level <= FINEST_COPY_LEVEL {
        if let Some((far, direction)) = copy_direction(content, tile) {
            return Some(Placement::Copied { far, direction, masked_children: 0 });
        }
    }
    if tile.level > FINEST_MASKING_LEVEL {
        return None;
    }
    let children_values = content.patterns.children_values(tile);
    masking_copy(content, tile, children_values, bound_above).or_else(|| masking_bind(children_values, bound_above))
}

/// What `tile` holds, if every cell of it agrees: its pattern number
/// says, down to 4x4; a 2x2, finer than patterns go, is read off its
/// four cells.
fn homogeneous_value(content: &Content, tile: Tile) -> Option<bool> {
    if tile.level <= FINEST_COPY_LEVEL {
        return content.patterns.homogeneous_value(tile);
    }
    let side = tile.side_in_cells();
    match content.bitmap.small_square(tile.top_left_cell(), side) {
        0 => Some(false),
        cells if cells.count_ones() as usize == side * side => Some(true),
        _ => None,
    }
}

/// A bind of a tile to the value not bound above, masking the children
/// not homogeneous with it, if it says at least
/// [`MIN_UNMASKED_CHILDREN_OF_A_MASKING_BIND`] of them; `children_values`
/// what each child holds, in reading order, if every cell of it agrees.
fn masking_bind(children_values: [Option<bool>; 4], bound_above: bool) -> Option<Placement> {
    let value = !bound_above;
    let mut masked_children = 0u8;
    for (index, &child_value) in children_values.iter().enumerate() {
        if child_value != Some(value) {
            masked_children |= 1 << index;
        }
    }
    let unmasked = children_values.len() as u32 - masked_children.count_ones();
    (unmasked >= MIN_UNMASKED_CHILDREN_OF_A_MASKING_BIND).then_some(Placement::Bound { value, masked_children })
}

/// The copy of `tile` that says the most of its children, masking the
/// rest, if it says enough of them to be worth it: near before far,
/// then in direction order, on a tie. A child is said when it holds the
/// same cells as the same child of the copy's source. `children_values`
/// is what each child holds, in reading order, if every cell of it
/// agrees.
///
/// A copy is checked a child at a time, and dropped as soon as the
/// children left could no longer make it worth it, or make it say more
/// than the best so far -- what they could add is known before any is
/// checked, from which are homogeneous. So is whether any copy could be
/// worth it at all.
fn masking_copy(content: &Content, tile: Tile, children_values: [Option<bool>; 4], bound_above: bool) -> Option<Placement> {
    let children = tile.children();
    // What each child would add, said: to the children said that are not
    // the value bound above, and to those not homogeneous.
    let adds_unmasked = children_values.map(|value| (value != Some(bound_above)) as u32);
    let adds_non_homogeneous = children_values.map(|value| value.is_none() as u32);
    let could_be_worth_it = |unmasked: u32, non_homogeneous: u32| {
        unmasked >= MIN_UNMASKED_CHILDREN || non_homogeneous >= MIN_UNMASKED_NON_HOMOGENEOUS_CHILDREN
    };
    let (all_unmasked, all_non_homogeneous) = (adds_unmasked.iter().sum(), adds_non_homogeneous.iter().sum());
    if !could_be_worth_it(all_unmasked, all_non_homogeneous) {
        return None;
    }
    let numbers = content.patterns.children_numbers(tile);
    let mut best: Option<(u32, Placement)> = None;
    for far in [false, true] {
        'direction: for direction in directions() {
            let offset = content.offsets.offset(far, direction);
            if tile.offset_by(offset).is_none() {
                continue;
            }
            // A child's source is its same child in the source tile.
            let child_offset = child_offset(offset);
            let (mut masked_children, mut unmasked, mut non_homogeneous) = (0u8, 0, 0);
            let (mut unmasked_left, mut non_homogeneous_left) = (all_unmasked, all_non_homogeneous);
            for index in 0..children.len() {
                unmasked_left -= adds_unmasked[index];
                non_homogeneous_left -= adds_non_homogeneous[index];
                if matches_at(content.patterns, children[index], numbers[index], child_offset) {
                    unmasked += adds_unmasked[index];
                    non_homogeneous += adds_non_homogeneous[index];
                } else {
                    masked_children |= 1 << index;
                }
                let most_possible = unmasked + unmasked_left;
                let beats_best = best.is_none_or(|(most, _)| most_possible > most);
                if !beats_best || !could_be_worth_it(most_possible, non_homogeneous + non_homogeneous_left) {
                    continue 'direction;
                }
            }
            // Every child checked and still possible: worth it, and more
            // than the best so far.
            best = Some((unmasked, Placement::Copied { far, direction, masked_children }));
        }
    }
    best.map(|(_, placement)| placement)
}

/// Which direction a tile copies from, if any, and whether it reads
/// from the far offsets rather than the near ones: near first.
fn copy_direction(content: &Content, tile: Tile) -> Option<(bool, u8)> {
    [false, true].into_iter().find_map(|far| matching_direction(content.patterns, content.offsets, tile, far).map(|direction| (far, direction)))
}
