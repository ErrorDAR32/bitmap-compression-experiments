//! The greedy tiler, the first pass: one rule, asked of every tile size
//! from the whole bitmap down to single cells, coarsest first, skipping
//! anything a coarser tile already claimed:
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
//! taken immediately. Cells are always homogeneous, so the pass always
//! covers the whole bitmap. Which binds are left to the binding above
//! is the tree's to say, not this pass's: a bind stays a bind here, for
//! the complex tiler to unmask if that is cheaper.
//!
//! A 2x2 is only ever asked whether it is homogeneous: if not, its four
//! cells are placed as 1x1 tiles, which the residual pass says.

use crate::gct::pyramids::copyable::{Copyable, FAR_DISTANCE, NEAR_DISTANCE};
use crate::gct::pyramids::homogeneity::Homogeneity;
use crate::gct::pyramids::placements::{binding_above, Placement, Placements, FINEST_MASKING_LEVEL};
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{directions, same_cells, Tile, CELL_LEVEL, CHILDREN_ACROSS};
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

/// Places tiles over one bitmap, biggest first; what it placed is a
/// [complex tiling pyramid](crate::gct::pyramids::complex_tiling) with
/// only its [placement](crate::gct::pyramids::placements) bits set. Reads the
/// bitmap's own homogeneity and copyable pyramids, and the bitmap itself
/// only to find which direction a copyable tile copies from.
pub fn greedy_tiler(bitmap: &Bitmap, homogeneity: &Pyramid, copyable: &Pyramid) -> Pyramid {
    let mut claimed = Bitmap::new();
    let mut placements = Pyramid::placements();

    for level in 0..=CELL_LEVEL {
        for tile in Tile::all_of_level(level) {
            // Tiles are placed biggest first and never overlap, so a
            // claimed corner means a coarser tile covers all of this
            // one -- same-size tiles partition the plane, and nothing
            // smaller has run yet.
            if tile.top_left_value(&claimed) {
                continue;
            }
            let bound_above = binding_above(tile, |at| placements.placement(at));
            let placement = if let Some(value) = homogeneity.homogeneous_value(tile) {
                Placement::bound(value)
            } else if let Some((far, direction)) = copyable.holds(tile).then(|| copy_direction(copyable, bitmap, tile)).flatten() {
                Placement::Copied { far, direction, masked_children: 0 }
            } else if let Some(placement) = (tile.level <= FINEST_MASKING_LEVEL).then(|| masking_copy(bitmap, homogeneity, tile, bound_above)).flatten() {
                placement
            } else if let Some(placement) = (tile.level <= FINEST_MASKING_LEVEL).then(|| masking_bind(homogeneity, tile, bound_above)).flatten() {
                placement
            } else {
                continue;
            };
            claim(tile, placement, &mut claimed);
            placements.place(tile, placement);
        }
    }
    placements
}

/// A bind of `tile` to the value not bound above, masking the children not
/// homogeneous with it, if it says at least
/// [`MIN_UNMASKED_CHILDREN_OF_A_MASKING_BIND`] of them.
fn masking_bind(homogeneity: &Pyramid, tile: Tile, bound_above: bool) -> Option<Placement> {
    let value = !bound_above;
    let mut masked_children = 0u8;
    for child in tile.children() {
        if homogeneity.homogeneous_value(child) != Some(value) {
            masked_children |= 1 << child.child_index();
        }
    }
    let unmasked = tile.children().len() as u32 - masked_children.count_ones();
    (unmasked >= MIN_UNMASKED_CHILDREN_OF_A_MASKING_BIND).then_some(Placement::Bound { value, masked_children })
}

/// Claims the cells a placed tile says: all of them, but for the
/// children it masks.
fn claim(tile: Tile, placement: Placement, claimed: &mut Bitmap) {
    if !placement.masks_any() {
        tile.set_in(claimed);
        return;
    }
    for child in tile.children() {
        if !placement.masks(child) {
            child.set_in(claimed);
        }
    }
}

/// The copy of `tile` that says the most of its children, masking the
/// rest, if it says enough of them to be worth it: near before far,
/// then in direction order, on a tie. A child is said when it holds the
/// same cells as the same child of the copy's source.
fn masking_copy(bitmap: &Bitmap, homogeneity: &Pyramid, tile: Tile, bound_above: bool) -> Option<Placement> {
    let mut best: Option<(u32, Placement)> = None;
    for (far, distance) in [(false, NEAR_DISTANCE), (true, FAR_DISTANCE)] {
        for direction in directions() {
            if tile.neighbour_at(direction, distance).is_none() {
                continue;
            }
            // A child's source is its same child in the source tile: the
            // tile's distance, counted in child sides.
            let child_distance = distance * CHILDREN_ACROSS as usize;
            let mut masked_children = 0u8;
            for child in tile.children() {
                let source = child.neighbour_at(direction, child_distance).expect("inside the source tile");
                if !same_cells(bitmap, child, source) {
                    masked_children |= 1 << child.child_index();
                }
            }
            let unmasked: Vec<Tile> =
                tile.children().into_iter().filter(|&child| masked_children & (1 << child.child_index()) == 0).collect();
            let non_homogeneous =
                unmasked.iter().filter(|&&child| homogeneity.homogeneous_value(child).is_none()).count() as u32;
            let unmasked = unmasked.iter().filter(|&&child| homogeneity.homogeneous_value(child) != Some(bound_above)).count() as u32;
            let worth_it = unmasked >= MIN_UNMASKED_CHILDREN || non_homogeneous >= MIN_UNMASKED_NON_HOMOGENEOUS_CHILDREN;
            if worth_it && best.is_none_or(|(most, _)| unmasked > most) {
                best = Some((unmasked, Placement::Copied { far, direction, masked_children }));
            }
        }
    }
    best.map(|(_, placement)| placement)
}

/// Which direction a tile copies from, if any, and whether that is a
/// far copy (a same-size neighbour of the tile's parent, at the tile's
/// own child position within it) rather than a near one (a same-size
/// neighbour of the tile itself).
fn copy_direction(copyable: &Pyramid, bitmap: &Bitmap, tile: Tile) -> Option<(bool, u8)> {
    if copyable.near_copyable(tile) {
        let near = directions()
            .find(|&direction| tile.neighbour(direction).is_some_and(|beside| same_cells(bitmap, tile, beside)));
        if let Some(direction) = near {
            return Some((false, direction));
        }
    }
    if !copyable.far_copyable(tile) {
        return None;
    }
    directions().find_map(|direction| {
        let far = tile.neighbour_at(direction, FAR_DISTANCE)?;
        same_cells(bitmap, tile, far).then_some((true, direction))
    })
}
