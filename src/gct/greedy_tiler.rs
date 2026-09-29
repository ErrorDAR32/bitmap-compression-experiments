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

use crate::gct::complex_tiler::bit_cost::own_bits;
use crate::gct::grammar::START_LEVEL_WIDTH;
use crate::gct::last_pass::Pricing;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::residual_prices::ResidualPrices;
use crate::gct::pyramids::copyable::{matches_at, CopyOffsets, FINEST_COPY_LEVEL};
use crate::gct::pyramids::placements::{Placement, BOUND_AT_THE_TOP, FINEST_MASKING_LEVEL};
use crate::gct::pyramids::patterns::{value_of, Patterns};
use crate::gct::tile::{cells_in_tile, directions, Tile, FINEST_PLACED_LEVEL, FLOOR_LEVEL};
use crate::morton::morton_index;
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

/// What the greedy tiler writes, whatever it held before.
pub struct GreedyTiling<'a> {
    /// What it placed: a
    /// [complex tiling pyramid](crate::gct::pyramids::complex_tiling)
    /// with only its [placement](crate::gct::pyramids::placements) bits
    /// set.
    pub placements: &'a mut ComplexTiling,
    /// What the last pass takes for each residual block it leaves --
    /// every 4x4 it reaches and places nothing at -- priced as the walk
    /// reaches it, in Morton order, the last pass's.
    pub residual_prices: &'a mut ResidualPrices,
}

/// Places tiles over one bitmap, biggest first, into `tiling`, and
/// counts the tree they make on its way back up. Reads only the
/// bitmap's content: which tiles are homogeneous and which match which,
/// from its patterns -- copies reading from `offsets` -- and a 2x2's
/// cells, finer than patterns go.
pub fn greedy_tiler(bitmap: &Bitmap, patterns: &Patterns, offsets: &CopyOffsets, tiling: &mut GreedyTiling) -> GreedyTreeBits {
    tiling.placements.clear();
    let mut walk = Walk { content: Content { bitmap, patterns, offsets }, tiling, pricing: Pricing::new(), tree_bits: GreedyTreeBits::new() };
    let whole_bitmap = Tile::whole_bitmap();
    place_at_or_under(&mut walk, Visit { tile: whole_bitmap, number: patterns.number(whole_bitmap), bound_above: BOUND_AT_THE_TOP, in_divide: false });
    walk.tree_bits
}

/// The bits the greedy tiler's own tree takes, its residual blocks at
/// their prices, counted node by node on the walk back up: what the
/// complex tiler's reference count ([`crate::gct::complex_tiler::bit_cost`])
/// gives it, which debug builds hold it to.
pub struct GreedyTreeBits {
    /// Every node's own bits, by its level.
    by_level: [u64; FLOOR_LEVEL as usize + 1],
    /// The coarsest level a node that does not divide whole is at: the
    /// tree's start level. Above it, every tile divides whole.
    start_level: u8,
}

impl GreedyTreeBits {
    /// No node counted.
    fn new() -> Self {
        Self { by_level: [0; FLOOR_LEVEL as usize + 1], start_level: FLOOR_LEVEL }
    }

    /// The tree's start level.
    pub fn start_level(&self) -> u8 {
        self.start_level
    }

    /// The bits of the whole tree: the start level, then every node from
    /// it down.
    pub fn bits(&self) -> u64 {
        START_LEVEL_WIDTH as u64 + self.by_level[self.start_level as usize..].iter().sum::<u64>()
    }
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

/// One walk's reading, writing and counting.
struct Walk<'a, 'b> {
    /// What it reads.
    content: Content<'a>,
    /// What it writes.
    tiling: &'a mut GreedyTiling<'b>,
    /// The residual blocks' pricing, block after block.
    pricing: Pricing,
    /// The tree's bits, so far.
    tree_bits: GreedyTreeBits,
}

/// The walk visits the 4x4 floor apart: copies reach down to it, and
/// masking stops just above it, so a 4x4 is only ever bound whole or
/// copied, and every coarser tile may be anything.
const _: () = assert!(FINEST_COPY_LEVEL == FLOOR_LEVEL && FINEST_MASKING_LEVEL + 1 == FLOOR_LEVEL && FLOOR_LEVEL + 1 == FINEST_PLACED_LEVEL);

/// A tile the greedy tiler visits, and what it knows of it on the way
/// down: its pattern number, read with its siblings' by its parent, and
/// the value bound above it.
#[derive(Clone, Copy)]
struct Visit {
    /// The tile.
    tile: Tile,
    /// Its pattern number.
    number: u16,
    /// The value bound above it.
    bound_above: bool,
    /// Whether it is a child of a divide -- a tile nothing is placed at
    /// -- and so, bound whole to the value bound above it, left to that
    /// binding: no node.
    in_divide: bool,
}

/// Places at the visited tile, or leaves it to its children, then does
/// the same for every child nothing placed here says; on the way back
/// up, prices it if it is a residual block and counts its node. Whether
/// it is left to the binding above.
fn place_at_or_under(walk: &mut Walk, visit: Visit) -> bool {
    let tile = visit.tile;
    if tile.level == FLOOR_LEVEL {
        let placed = floor_placement(&walk.content, visit);
        if placed.is_none() {
            place_2x2s(walk.content.bitmap, tile, walk.tiling.placements);
            walk.pricing.price(walk.content.bitmap, morton_index(tile.x, tile.y), walk.tiling.residual_prices);
        }
        walk.tiling.placements.record_placed(tile, placed, visit.bound_above);
        return count_node(walk, visit, NO_CHILD_LEFT);
    }
    // Every child's number, one lookup: four consecutive elements.
    let children_numbers = walk.content.patterns.children_numbers(tile);
    let placed = placement(&walk.content, visit, children_numbers);
    let bound_inside = placed.map_or(visit.bound_above, |placement| placement.bound_inside(visit.bound_above));
    let mut children_left = NO_CHILD_LEFT;
    for (index, child) in tile.children().into_iter().enumerate() {
        if placed.is_none_or(|placement| placement.masks(child)) {
            let child_visit = Visit { tile: child, number: children_numbers[index], bound_above: bound_inside, in_divide: placed.is_none() };
            if place_at_or_under(walk, child_visit) {
                children_left |= 1 << index;
            }
        }
    }
    walk.tiling.placements.record_placed(tile, placed, visit.bound_above);
    count_node(walk, visit, children_left)
}

/// No child left to the binding above.
const NO_CHILD_LEFT: u8 = 0;

/// Counts the visited tile's node, placed as it is and `children_left`
/// the children it leaves to the binding above -- nothing, if it is
/// itself left to the binding above -- and says whether it is.
fn count_node(walk: &mut Walk, visit: Visit, children_left: u8) -> bool {
    let tile = visit.tile;
    let here = walk.tiling.placements.fields(tile);
    let left = visit.in_divide && here.left_to_binding_above(tile, visit.bound_above, &NestedResolutions::none());
    if !left {
        walk.tree_bits.by_level[tile.level as usize] += own_bits(tile, here, children_left, walk.tiling.residual_prices);
    }
    let divides_whole = here.placed().is_none() && tile.level < FLOOR_LEVEL && children_left == NO_CHILD_LEFT;
    if !divides_whole {
        walk.tree_bits.start_level = walk.tree_bits.start_level.min(tile.level);
    }
    left
}

/// Binds each of the 4x4 `tile`'s 2x2s that is homogeneous, read off
/// the 4x4's cells, one run of the bitmap whose four quarters are its
/// 2x2s in reading order. A 2x2 is only ever asked that: nothing finer
/// than a 2x2 is placed, and no 2x2 copies or masks.
fn place_2x2s(bitmap: &Bitmap, tile: Tile, placements: &mut ComplexTiling) {
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
fn floor_placement(content: &Content, visit: Visit) -> Option<Placement> {
    if let Some(value) = value_of(visit.number) {
        return Some(Placement::bound(value));
    }
    copy_direction(content, visit).map(|(far, direction)| Placement::Copied { far, direction, masked_children: 0 })
}

/// What the rule places at the visited tile, coarser than 4x4, if
/// anything; `children_numbers` its children's pattern numbers.
fn placement(content: &Content, visit: Visit, children_numbers: [u16; 4]) -> Option<Placement> {
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
