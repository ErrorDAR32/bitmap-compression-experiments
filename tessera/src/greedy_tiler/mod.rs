//! The greedy tiler, the one pass over a bitmap's tiles: it places
//! tiles, biggest first, on its way down, and on its way back up prices
//! the residual blocks, turns a tile into one complex tile where that
//! takes fewer bits than the nodes under it, and counts the tree.
//!
//! On the way down, one rule ([`rule`]), asked of the whole bitmap, then
//! of every tile nothing coarser says, down to 2x2s:
//!
//! 1. Homogeneous? Bind it.
//! 2. Down to 4x4, copyable (a same-size neighbour, or, one level up, a
//!    same-size neighbour of the tile's own parent)? Copy it.
//! 3. Down to 8x8, does a copy say at least
//!    [`MIN_UNMASKED_CHILDREN`] of its
//!    children, or at least
//!    [`MIN_UNMASKED_NON_HOMOGENEOUS_CHILDREN`]
//!    that are not homogeneous? Copy it, masking the others.
//! 4. Down to 8x8, are at least
//!    [`MIN_UNMASKED_CHILDREN_OF_A_MASKING_BIND`]
//!    children homogeneous with the value not bound above? Bind it to
//!    that value, masking the others: every child it leaves unnamed, at
//!    any depth, is bound by it. Clear is bound at the top.
//! 5. Else leave it for its four children to try for themselves.
//!
//! Masked children are left to the tiles placed inside them. A tile that
//! qualifies is taken immediately. What a tile gets depends only on its
//! own cells and on what its ancestors got, so the pass walks down depth
//! first, carrying the value bound above, into the children of a tile
//! left unplaced and the children a placed tile masks. Cells are always
//! homogeneous, so the pass always covers the whole bitmap.
//!
//! A 2x2 is only ever asked whether it is homogeneous: if not, nothing
//! is placed in it -- its cells are said in the last pass or by a
//! complex tile of 1x1 resolution -- so nothing finer than a 2x2 is ever
//! placed. The tree goes no finer than 4x4: a 2x2 placed is read only
//! by a complex tile of 2x2 resolution.
//!
//! On the way back up, every tile the walk reached is counted after its
//! children: its bits as placed, its own node and each child node's
//! fewest bits, a residual block at its price. A tile nothing is placed
//! at may instead be one complex tile ([`complex_tiles`]), saying every
//! cell under it; it is made one where that takes fewer bits, and
//! whatever is under it is no longer read. Tiles that do not overlap
//! cost bits independently, so each tile's fewest bits, found from its
//! children's, are the fewest the whole tree can take from the tiles
//! placed. Every count follows the grammar's widths, as
//! [`crate::bit_cost`] spells them out for any tiling; debug builds
//! hold every count at 16x16 and finer to it.

pub mod complex_tiles;
pub mod rule;

use crate::bit_cost::{bits_with, divide_bits, own_bits, Counting};
use crate::grammar::START_LEVEL_WIDTH;
use crate::last_pass::Pricing;
use crate::pyramids::complex_tiling::ComplexTiling;
use crate::pyramids::copyable::{CopyOffsets, FINEST_COPY_LEVEL};
use crate::pyramids::patterns::Patterns;
use crate::pyramids::placements::{BOUND_AT_THE_TOP, FINEST_MASKING_LEVEL};
use crate::residual_prices::ResidualPrices;
use crate::set_counts::SetCounts;
use crate::tile::{tiles_in_level, Tile, CELL_LEVEL, FINEST_PLACED_LEVEL, FLOOR_LEVEL};
use crate::morton::morton_index;
use crate::Bitmap;
use complex_tiles::best_complex_tile;
use rule::{floor_placement, place_2x2s, placement};

pub use rule::{MIN_UNMASKED_CHILDREN, MIN_UNMASKED_CHILDREN_OF_A_MASKING_BIND, MIN_UNMASKED_NON_HOMOGENEOUS_CHILDREN};

/// What the greedy tiler writes, whatever it held before.
pub struct GreedyTiling<'a> {
    /// The [complex tiling pyramid](crate::pyramids::complex_tiling):
    /// what it placed, and the complex tiles it made.
    pub complex_tiling: &'a mut ComplexTiling,
    /// What the last pass takes for each residual block it leaves --
    /// every 4x4 it reaches and places nothing at -- priced as the walk
    /// reaches it, in Morton order, the last pass's.
    pub residual_prices: &'a mut ResidualPrices,
}

/// Tiles one bitmap into `tiling`, and counts the tree it makes. Reads
/// only the bitmap's content: which tiles are homogeneous and which
/// match which, from its patterns, a 2x2's cells, finer than patterns
/// go, and a cell list's set cells.
pub fn greedy_tiler(content: Content, tiling: &mut GreedyTiling) -> TreeBits {
    tiling.complex_tiling.clear();
    let patterns = content.patterns;
    let mut walk = Walk { content, tiling, pricing: Pricing::new() };
    let whole_bitmap = Tile::whole_bitmap();
    let whole = place_at_or_under(&mut walk, Visit { tile: whole_bitmap, number: patterns.number(whole_bitmap), bound_above: BOUND_AT_THE_TOP, in_divide: false });
    TreeBits::of_whole_bitmap(whole)
}

/// The bits the tree takes, its residual blocks at their prices, as the
/// greedy tiler counted it: what the reference count
/// ([`crate::bit_cost`]) gives it, which debug builds hold it to.
#[derive(Clone, Copy, Debug)]
pub struct TreeBits {
    /// The bits.
    bits: u64,
    /// The tree's start level: that of its coarsest node that does not
    /// divide whole.
    start_level: u8,
}

impl TreeBits {
    /// The tree's bits from the whole bitmap's count: the start level,
    /// then every node from it down -- the whole bitmap's fewest bits,
    /// less the divides above the start level, which are not written.
    fn of_whole_bitmap(whole: Visited) -> Self {
        let trunk: u64 = (0..whole.start_level).map(|level| tiles_in_level(level) as u64 * divide_bits(level, false)).sum();
        Self { bits: START_LEVEL_WIDTH as u64 + whole.fewest_bits as u64 - trunk, start_level: whole.start_level }
    }

    /// The bits.
    pub fn bits(&self) -> u64 {
        self.bits
    }

    /// The tree's start level.
    pub fn start_level(&self) -> u8 {
        self.start_level
    }
}

/// What the greedy tiler reads of a bitmap: its cells, how many are set
/// before each word of them, and its patterns -- and where copies read
/// from.
pub struct Content<'a> {
    /// Its cells.
    pub bitmap: &'a Bitmap,
    /// How many of its cells are set before each word of them.
    pub set_counts: &'a SetCounts,
    /// Its patterns.
    pub patterns: &'a Patterns,
    /// Where copies read from.
    pub offsets: &'a CopyOffsets,
}

/// One walk's reading, writing and pricing.
struct Walk<'a, 'b> {
    /// What it reads.
    content: Content<'a>,
    /// What it writes.
    tiling: &'a mut GreedyTiling<'b>,
    /// The residual blocks' pricing, block after block.
    pricing: Pricing,
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

/// What the walk knows of a tile once it has been through it and
/// everything under it.
#[derive(Clone, Copy)]
struct Visited {
    /// Whether it is left to the binding above: no node, no bits.
    left: bool,
    /// The fewest bits it and everything under it take, as a node.
    fewest_bits: u32,
    /// The coarsest level of a node at or under it that does not divide
    /// whole, as it is said in those fewest bits.
    start_level: u8,
}

/// Places at the visited tile, or leaves it to its children, then does
/// the same for every child nothing placed here says; on the way back
/// up, prices it if it is a residual block, and counts it ([`count`]).
fn place_at_or_under(walk: &mut Walk, visit: Visit) -> Visited {
    let tile = visit.tile;
    let mut children = Children { left: NO_CHILD, fewest_bits: 0, start_level: FLOOR_LEVEL };
    let placed = if tile.level == FLOOR_LEVEL {
        let placed = floor_placement(&walk.content, visit);
        if placed.is_none() {
            place_2x2s(walk.content.bitmap, tile, walk.tiling.complex_tiling);
            walk.pricing.price(walk.content.bitmap, morton_index(tile.x, tile.y), walk.tiling.residual_prices);
        }
        placed
    } else {
        // Every child's number, one lookup: four consecutive elements.
        let children_numbers = walk.content.patterns.children_numbers(tile);
        let placed = placement(&walk.content, visit, children_numbers);
        let bound_inside = placed.map_or(visit.bound_above, |placement| placement.bound_inside(visit.bound_above));
        for (index, child) in tile.children().into_iter().enumerate() {
            if placed.is_none_or(|placement| placement.masks(child)) {
                let child_visit = Visit { tile: child, number: children_numbers[index], bound_above: bound_inside, in_divide: placed.is_none() };
                children.add(index, place_at_or_under(walk, child_visit));
            }
        }
        placed
    };
    walk.tiling.complex_tiling.record_placed(tile, placed, visit.bound_above);
    count(walk, visit, children)
}

/// What the walk knows of a tile's children once it has been through
/// them.
struct Children {
    /// Which are left to the binding above, bit `i` for child `i`.
    left: u8,
    /// The fewest bits of those that are nodes, added up.
    fewest_bits: u32,
    /// The coarsest start level among them.
    start_level: u8,
}

/// No child left to the binding above.
const NO_CHILD: u8 = 0;

impl Children {
    /// Adds the child at `index` in reading order, `visited`.
    fn add(&mut self, index: usize, visited: Visited) {
        self.left |= (visited.left as u8) << index;
        if !visited.left {
            self.fewest_bits += visited.fewest_bits;
        }
        self.start_level = self.start_level.min(visited.start_level);
    }
}

/// Counts the visited tile, placed as it is, from `children`: its fewest
/// bits -- its node as placed and its children's fewest, or, where it
/// takes fewer, one complex tile, which it is then made.
fn count(walk: &mut Walk, visit: Visit, children: Children) -> Visited {
    let tile = visit.tile;
    let here = walk.tiling.complex_tiling.fields(tile);
    let own = own_bits(tile, here, children.left, walk.tiling.residual_prices) as u32;
    let divides_whole = here.placed().is_none() && tile.level < FLOOR_LEVEL && children.left == NO_CHILD;
    let mut visited = Visited {
        left: visit.in_divide && here.left_to_binding_above(tile, visit.bound_above),
        fewest_bits: own + children.fewest_bits,
        start_level: if divides_whole { children.start_level } else { tile.level },
    };
    if here.placed().is_none() {
        if let Some(complex) = best_complex_tile(walk.content.bitmap, walk.content.set_counts, tile, here, visited.fewest_bits) {
            walk.tiling.complex_tiling.make_complex_tile(tile, complex.size_offset, complex.cell_list);
            (visited.fewest_bits, visited.start_level) = (complex.bits, tile.level);
        }
    }
    debug_assert_matches_reference(walk, tile, visited.fewest_bits);
    visited
}

/// The finest tiles debug builds check against the reference count,
/// 16x16 and finer: coarser ones would count too much of the bitmap
/// again to check every one.
const FINEST_CHECKED_LEVEL: u8 = CELL_LEVEL - 4;

/// In debug builds, for a tile of 16x16 or finer, that `fewest_bits` is
/// what the reference count gives it as the tiling now stands.
fn debug_assert_matches_reference(walk: &Walk, tile: Tile, fewest_bits: u32) {
    if cfg!(debug_assertions) && tile.level >= FINEST_CHECKED_LEVEL {
        let counting = Counting { complex_tiling: walk.tiling.complex_tiling, bitmap: walk.content.bitmap, residual_prices: walk.tiling.residual_prices };
        let here = counting.complex_tiling.fields(tile);
        let reference = bits_with(counting, tile, here, here.bound_above());
        assert_eq!(fewest_bits as u64, reference, "{tile:?}: the walk's count is not the reference count");
    }
}
