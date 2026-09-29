//! The greedy tiler's rule: what a tile gets placed at it, asked of
//! each tile the walk reaches, from what the walk knows of it -- its
//! pattern number and the value bound above it -- and the patterns.

use super::{Content, Visit};
use crate::pyramids::complex_tiling::ComplexTiling;
use crate::pyramids::copyable::matches_at;
use crate::pyramids::placements::Placement;
use crate::pyramids::patterns::value_of;
use crate::tile::{cells_in_tile, directions, Tile, FINEST_PLACED_LEVEL};
use crate::morton::morton_index;
use crate::Bitmap;

/// A masking copy costs about 10 bits before its masked children: a
/// copy, a mask-present bit, a 4-bit child mask. What it saves depends
/// on what the children it says would cost without it. A homogeneous
/// one is cheap anyway -- a tile, or 1 bit in a complex tile's payload,
/// which masking it away takes from the complex tile. Any other
/// needs a copy or a subtree of its own, 5 bits or more. So a masking
/// copy must say at least this many children...
pub const MIN_UNMASKED_CHILDREN: u32 = 3;
/// ...or at least this many that are not homogeneous. Either half alone
/// measured worse (`docs/tessera.md`).
pub const MIN_UNMASKED_NON_HOMOGENEOUS_CHILDREN: u32 = 2;

/// A masking bind, spelled as a divide that flips the value bound above,
/// costs 7 bits before its masked children; each child it says would
/// otherwise be a tile of its own, 4 bits or more. So it must say at
/// least this many children.
pub const MIN_UNMASKED_CHILDREN_OF_A_MASKING_BIND: u32 = 2;

/// Binds each of the 4x4 `tile`'s 2x2s that is homogeneous, read off
/// the 4x4's cells, one run of the bitmap whose four quarters are its
/// 2x2s in reading order. A 2x2 is only ever asked that: nothing finer
/// than a 2x2 is placed, and no 2x2 copies or masks.
pub(super) fn place_2x2s(bitmap: &Bitmap, tile: Tile, placements: &mut ComplexTiling) {
    let cells = bitmap.morton_run(morton_index(tile.x, tile.y) * TILE_CELLS, TILE_CELLS);
    placements.bind_children(
        tile,
        std::array::from_fn(|index| match cells >> (index * CHILD_CELLS) & CHILD_MASK {
            0 => Some(false),
            CHILD_MASK => Some(true),
            _ => None,
        }),
    );
}

/// A 4x4's cells...
const TILE_CELLS: usize = cells_in_tile(FINEST_PLACED_LEVEL - 1) as usize;
/// ...and each 2x2's, one quarter of them.
const CHILD_CELLS: usize = cells_in_tile(FINEST_PLACED_LEVEL) as usize;
/// A 2x2's cells, all set.
const CHILD_MASK: u64 = (1 << CHILD_CELLS) - 1;

/// What the rule places at the visited 4x4, if anything: a whole bind or
/// a copy -- nothing masks at 4x4.
pub(super) fn floor_placement(content: &Content, visit: Visit) -> Option<Placement> {
    if let Some(value) = value_of(visit.number) {
        return Some(Placement::bound(value));
    }
    copy_direction(content, visit).map(|(far, direction)| Placement::Copied { far, direction, masked_children: 0 })
}

/// What the rule places at the visited tile, coarser than 4x4, if
/// anything; `children_numbers` its children's pattern numbers.
pub(super) fn placement(content: &Content, visit: Visit, children_numbers: [u16; 4]) -> Option<Placement> {
    if let Some(value) = value_of(visit.number) {
        return Some(Placement::bound(value));
    }
    if let Some((far, direction)) = copy_direction(content, visit) {
        return Some(Placement::Copied { far, direction, masked_children: 0 });
    }
    masking_copy(content, visit, children_numbers).or_else(|| masking_bind(children_numbers, visit.bound_above))
}

/// A bind of a tile to the value not bound above, masking the children
/// not homogeneous with it, if it says at least
/// [`MIN_UNMASKED_CHILDREN_OF_A_MASKING_BIND`] of them; `children_numbers`
/// its children's pattern numbers, in reading order.
fn masking_bind(children_numbers: [u16; 4], bound_above: bool) -> Option<Placement> {
    let children_values = children_numbers.map(value_of);
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
/// same cells as the same child of the copy's source -- read, for all
/// four at once, off the source's children's numbers, four consecutive
/// elements in Morton order. `numbers` are the visited tile's
/// children's pattern numbers, in reading order.
fn masking_copy(content: &Content, visit: Visit, numbers: [u16; 4]) -> Option<Placement> {
    let (tile, bound_above) = (visit.tile, visit.bound_above);
    let children_values = numbers.map(value_of);
    let child_level = tile.level + 1;
    // Only a child whose pattern another tile holds can be said.
    let can_match: [bool; 4] = std::array::from_fn(|index| content.patterns.repeats(child_level, numbers[index]));
    // What each child adds, said: to the children said that are not the
    // value bound above, and to those not homogeneous.
    let adds_unmasked: [u32; 4] = std::array::from_fn(|index| (can_match[index] && children_values[index] != Some(bound_above)) as u32);
    let adds_non_homogeneous: [u32; 4] = std::array::from_fn(|index| (can_match[index] && children_values[index].is_none()) as u32);
    let worth_it = |unmasked: u32, non_homogeneous: u32| {
        unmasked >= MIN_UNMASKED_CHILDREN || non_homogeneous >= MIN_UNMASKED_NON_HOMOGENEOUS_CHILDREN
    };
    if !worth_it(adds_unmasked.iter().sum(), adds_non_homogeneous.iter().sum()) {
        return None;
    }
    let mut best: Option<(u32, Placement)> = None;
    for far in [false, true] {
        for direction in directions() {
            let Some(source) = tile.offset_by(content.offsets.offset(far, direction)) else { continue };
            let source_numbers = content.patterns.children_numbers(source);
            let (mut masked_children, mut unmasked, mut non_homogeneous) = (0u8, 0, 0);
            for index in 0..numbers.len() {
                if can_match[index] && source_numbers[index] == numbers[index] {
                    unmasked += adds_unmasked[index];
                    non_homogeneous += adds_non_homogeneous[index];
                } else {
                    masked_children |= 1 << index;
                }
            }
            if worth_it(unmasked, non_homogeneous) && best.is_none_or(|(most, _)| unmasked > most) {
                best = Some((unmasked, Placement::Copied { far, direction, masked_children }));
            }
        }
    }
    best.map(|(_, placement)| placement)
}

/// Which direction the visited tile copies from, if any, and whether it
/// reads from the far offsets rather than the near ones: near first.
fn copy_direction(content: &Content, visit: Visit) -> Option<(bool, u8)> {
    if !content.patterns.repeats(visit.tile.level, visit.number) {
        return None;
    }
    [false, true].into_iter().find_map(|far| {
        directions()
            .find(|&direction| matches_at(content.patterns, visit.tile, visit.number, content.offsets.offset(far, direction)))
            .map(|direction| (far, direction))
    })
}
