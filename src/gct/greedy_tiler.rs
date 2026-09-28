//! The greedy tiler, the first pass: one rule, asked of the whole bitmap,
//! then of every tile nothing coarser says, down to single cells:
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
//! unplaced and the children a placed tile masks. Cells are always homogeneous, so the pass always
//! covers the whole bitmap. Which binds are left to the binding above
//! is the tree's to say, not this pass's: a bind stays a bind here, for
//! the complex tiler to unmask if that is cheaper.
//!
//! A 2x2 is only ever asked whether it is homogeneous: if not, its four
//! cells are placed as 1x1 tiles, which the residual pass says.

use crate::gct::pyramids::copyable::{matching_direction, matching_directions, FAR_DISTANCE, FINEST_COPY_LEVEL, NEAR_DISTANCE};
use crate::gct::pyramids::homogeneity::Homogeneity;
use crate::gct::pyramids::placements::{Placement, Placements, BOUND_AT_THE_TOP, FINEST_MASKING_LEVEL};
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{directions, Tile, CHILDREN_ACROSS};
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
/// only its [placement](crate::gct::pyramids::placements) bits set. Reads
/// only the bitmap's content: which tiles are homogeneous, from its
/// homogeneity pyramid, and which match which, asked of the bitmap only
/// for tiles that are not.
pub fn greedy_tiler(bitmap: &Bitmap, homogeneity: &Pyramid) -> Pyramid {
    let mut placements = Pyramid::placements();
    let content = Content { bitmap, homogeneity };
    place_at_or_under(&content, Tile::whole_bitmap(), BOUND_AT_THE_TOP, &mut placements);
    placements
}

/// What the greedy tiler reads: a bitmap, and its homogeneity pyramid.
struct Content<'a> {
    /// The bitmap tiled.
    bitmap: &'a Bitmap,
    /// Its homogeneity pyramid.
    homogeneity: &'a Pyramid,
}

impl Content<'_> {
    /// What `tile` holds, if every cell of it agrees.
    fn homogeneous_value(&self, tile: Tile) -> Option<bool> {
        self.homogeneity.homogeneous_value(tile)
    }

    /// See [`matching_directions`].
    fn matching_directions(&self, tile: Tile, distance: usize) -> u8 {
        matching_directions(self.homogeneity, self.bitmap, tile, distance)
    }

    /// See [`matching_direction`].
    fn matching_direction(&self, tile: Tile, distance: usize) -> Option<u8> {
        matching_direction(self.homogeneity, self.bitmap, tile, distance)
    }
}

/// Places `tile`, or leaves it to its children, then does the same for
/// every child nothing placed here says; `bound_above` the value bound
/// above `tile`.
fn place_at_or_under(content: &Content, tile: Tile, bound_above: bool, placements: &mut Pyramid) {
    let Some(placement) = placement(content, tile, bound_above) else {
        for child in tile.children() {
            place_at_or_under(content, child, bound_above, placements);
        }
        return;
    };
    placements.place(tile, placement);
    let bound_inside = match placement {
        Placement::Bound { value, .. } => value,
        Placement::Copied { .. } => bound_above,
    };
    for child in tile.children() {
        if placement.masks(child) {
            place_at_or_under(content, child, bound_inside, placements);
        }
    }
}

/// What the rule places at `tile`, if anything.
fn placement(content: &Content, tile: Tile, bound_above: bool) -> Option<Placement> {
    if let Some(value) = content.homogeneous_value(tile) {
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
    masking_copy(content, tile, bound_above).or_else(|| masking_bind(content, tile, bound_above))
}

/// A bind of `tile` to the value not bound above, masking the children not
/// homogeneous with it, if it says at least
/// [`MIN_UNMASKED_CHILDREN_OF_A_MASKING_BIND`] of them.
fn masking_bind(content: &Content, tile: Tile, bound_above: bool) -> Option<Placement> {
    let value = !bound_above;
    let mut masked_children = 0u8;
    for child in tile.children() {
        if content.homogeneous_value(child) != Some(value) {
            masked_children |= 1 << child.child_index();
        }
    }
    let unmasked = tile.children().len() as u32 - masked_children.count_ones();
    (unmasked >= MIN_UNMASKED_CHILDREN_OF_A_MASKING_BIND).then_some(Placement::Bound { value, masked_children })
}

/// The copy of `tile` that says the most of its children, masking the
/// rest, if it says enough of them to be worth it: near before far,
/// then in direction order, on a tie. A child is said when it holds the
/// same cells as the same child of the copy's source.
fn masking_copy(content: &Content, tile: Tile, bound_above: bool) -> Option<Placement> {
    let children = tile.children();
    let homogeneous = children.map(|child| content.homogeneous_value(child));
    let mut best: Option<(u32, Placement)> = None;
    for (far, distance) in [(false, NEAR_DISTANCE), (true, FAR_DISTANCE)] {
        // A child's source is its same child in the source tile: the
        // tile's distance, counted in child sides.
        let child_distance = distance * CHILDREN_ACROSS as usize;
        let matching = children.map(|child| content.matching_directions(child, child_distance));
        for direction in directions() {
            if tile.neighbour_at(direction, distance).is_none() {
                continue;
            }
            let (mut masked_children, mut unmasked, mut non_homogeneous) = (0u8, 0, 0);
            for (index, (&matched, &value)) in matching.iter().zip(&homogeneous).enumerate() {
                if matched & 1 << direction == 0 {
                    masked_children |= 1 << index;
                    continue;
                }
                if value.is_none() {
                    non_homogeneous += 1;
                }
                if value != Some(bound_above) {
                    unmasked += 1;
                }
            }
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
/// neighbour of the tile itself): near first.
fn copy_direction(content: &Content, tile: Tile) -> Option<(bool, u8)> {
    let near = content.matching_direction(tile, NEAR_DISTANCE).map(|direction| (false, direction));
    near.or_else(|| content.matching_direction(tile, FAR_DISTANCE).map(|direction| (true, direction)))
}
