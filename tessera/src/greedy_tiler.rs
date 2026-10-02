//! The greedy tiler, two walks writing a bitmap's tree: top down, the
//! greedy tiling ([`greedy_tiling`]), tiles placed, biggest first; then
//! bottom up, the complex tiling ([`complex_tiling`]), every tile's
//! fewest bits counted, and a tile made one complex tile where that
//! takes fewer. The whole tree is counted to be weighed against the
//! binary count tree, and is thrown away, both walks, when that wins.
//!
//! The greedy tiling asks one rule of the whole bitmap, then of every
//! tile nothing coarser says, down to the 4x4 floor:
//!
//! 1. Homogeneous? Bind it.
//! 2. Copyable (a same-size tile at one of the copy offsets holds the
//!    same cells)? Copy it.
//! 3. Coarser than 4x4, does a copy copy at least
//!    [`MIN_COPIED_CHILDREN`] of its children, or at least
//!    [`MIN_COPIED_NON_HOMOGENEOUS_CHILDREN`] that are not homogeneous?
//!    Copy it, naming the others as nodes of their own.
//! 4. Coarser than 4x4, are at least [`MIN_FLIPPED_CHILDREN`] children
//!    homogeneous with the value not bound above? Make it a flipping
//!    divide: everything under it not named is bound to that value.
//!    Clear is bound at the top.
//! 5. Else divide it: its four children try for themselves.
//!
//! A child a divide leaves, bound whole to the value bound above it, is
//! no node: the binding above says it. A 4x4 nothing is placed at is a
//! residual block, priced as the last pass would code it.
//!
//! The complex tiling counts every tile after its children: its
//! node's own bits, as the grammar writes them, and each child node's
//! fewest. A divide or a residual block may instead be one complex tile
//! saying every cell under it -- at the one size every cell under it is
//! bound at, if any, or raw, or as a cell list -- made one where that
//! takes fewer bits. Tiles that do not overlap cost bits independently,
//! so each tile's fewest bits, found from its children's, are the
//! fewest the whole tree can take from the tiles placed.
//!
//! Making a tile one complex tile leaves the nodes already written under
//! it in the tree pyramid, stale: nothing reads under a complex tile.

use crate::payload_writer::cell_list_least_bits;
use crate::quadtree_writer::{node_bits, raw_resolution_fits, START_LEVEL_WIDTH};
use crate::tree::{start_level, Node, Tree, BOUND_AT_THE_TOP};
use crate::last_pass::Pricing;
use crate::patterns::{homogeneous_value_of, Patterns};
use crate::set_cells_before_each_word::SetCellsBeforeEachWord;
use crate::tile::{cells_in_tile, copy_offset, tiles_in_level, Tile, CELL_LEVEL, DIRECTIONS, FLOOR_LEVEL};
use bitmap::Bitmap;

/// A copy naming its children costs about 10 bits before them. A
/// homogeneous child it copies is cheap anyway; any other needs a copy
/// or a subtree of its own, 5 bits or more. So it must copy at least
/// this many children that are not the value bound above...
pub const MIN_COPIED_CHILDREN: u32 = 3;
/// ...or at least this many that are not homogeneous. Either half alone
/// measured worse.
pub const MIN_COPIED_NON_HOMOGENEOUS_CHILDREN: u32 = 2;
/// A flipping divide costs 7 bits before its named children; each child
/// it binds would otherwise be a tile of its own, 4 bits or more. So it
/// must bind at least this many.
pub const MIN_FLIPPED_CHILDREN: u32 = 2;

/// Every child named.
const ALL_CHILDREN: u8 = 0b1111;
/// A bind of the whole tile: a complex tile of its own size.
const PLAIN_TILE: Node = Node::ComplexTile { size_offset: 0 };

/// A tile the walk visits, and what it knows of it on the way down.
#[derive(Clone, Copy)]
struct Visit {
    /// The tile.
    tile: Tile,
    /// Its pattern number.
    pattern_number: u16,
    /// The value bound above it.
    bound_above: bool,
    /// Whether it is a child of a divide.
    child_of_divide: bool,
}

/// What the complex tiling knows of a tile once it is through everything
/// under it.
struct CountedSubtree {
    /// The fewest bits it and everything under it take.
    fewest_bits: u64,
    /// The one size every cell under it is bound at, if any.
    bound_size: Option<u8>,
}

/// The greedy tiling, the top-down pass: places tiles in `tree`, whatever
/// it held, from the whole bitmap down -- each tile's node, as placed,
/// and every child it names visited in turn; a child it does not name,
/// and one a divide leaves to the binding above, [`Node::Absent`].
pub fn greedy_tiling(patterns: &Patterns, tree: &mut Tree) {
    let whole = Tile::WHOLE_BITMAP;
    place_subtree(patterns, tree, Visit { tile: whole, pattern_number: patterns.number(whole), bound_above: BOUND_AT_THE_TOP, child_of_divide: false });
}

/// Places at the visited tile, or divides it, then visits every child it
/// names.
fn place_subtree(patterns: &Patterns, tree: &mut Tree, visit: Visit) {
    let tile = visit.tile;
    let children_numbers = (tile.level < FLOOR_LEVEL).then(|| patterns.children_numbers(tile));
    let unplaced = if tile.level == FLOOR_LEVEL { Node::Residual } else { Node::Divided };
    let (node, named) = place(patterns, visit, children_numbers).unwrap_or((unplaced, ALL_CHILDREN));
    let left_to_binding_above =
        visit.child_of_divide && node == PLAIN_TILE && homogeneous_value_of(visit.pattern_number) == Some(visit.bound_above);
    tree.set(tile, if left_to_binding_above { Node::Absent } else { node });
    let Some(numbers) = children_numbers else { return };
    let bound_inside = visit.bound_above != (node == Node::FlippingDivide);
    for (index, child) in tile.children().into_iter().enumerate() {
        if named >> index & 1 == 0 {
            tree.set(child, Node::Absent);
        } else {
            let child_visit = Visit { tile: child, pattern_number: numbers[index], bound_above: bound_inside, child_of_divide: node == Node::Divided };
            place_subtree(patterns, tree, child_visit);
        }
    }
}

/// The complex tiling, the bottom-up pass, over the greedy tiling in
/// `tree`: every tile counted after its children, its residual blocks
/// priced in `pricing`, and each divide or residual block made one
/// complex tile where that takes fewer bits. The bits the tree takes,
/// its residual blocks at their prices, and its start level.
pub fn complex_tiling(bitmap: &Bitmap, set_cells_before_each_word: &SetCellsBeforeEachWord, tree: &mut Tree, pricing: &mut Pricing) -> (u64, u8) {
    pricing.clear();
    let fewest_bits = count_subtree(bitmap, set_cells_before_each_word, tree, pricing, Tile::WHOLE_BITMAP).fewest_bits;
    // The divides above the start level are never written.
    let start_level = start_level(tree);
    let trunk: u64 = (0..start_level).map(|level| tiles_in_level(level) as u64 * node_bits(tree, bitmap, Tile { level, x: 0, y: 0 }, Node::Divided)).sum();
    (START_LEVEL_WIDTH as u64 + fewest_bits - trunk, start_level)
}

/// Counts `tile`'s node, placed, after every child that is a node, and
/// makes it one complex tile if that takes fewer bits.
fn count_subtree(bitmap: &Bitmap, set_cells_before_each_word: &SetCellsBeforeEachWord, tree: &mut Tree, pricing: &mut Pricing, tile: Tile) -> CountedSubtree {
    let node = tree.get(tile);
    let (mut fewest_bits, mut bound_size) = (0, None);
    if node == Node::Residual {
        fewest_bits = pricing.price(bitmap, tile);
        bound_size = all_2x2s_homogeneous(bitmap, tile).then_some(FLOOR_LEVEL + 1);
    } else if node == PLAIN_TILE {
        bound_size = Some(tile.level);
    } else if node.has_children() {
        let mut sizes = [None; 4];
        for (index, child) in tile.children().into_iter().enumerate() {
            if tree.get(child) == Node::Absent {
                // A divide's child left to the binding above is bound whole.
                sizes[index] = Some(child.level);
            } else {
                let counted = count_subtree(bitmap, set_cells_before_each_word, tree, pricing, child);
                fewest_bits += counted.fewest_bits;
                sizes[index] = counted.bound_size;
            }
        }
        if node == Node::Divided && sizes.iter().all(|&size| size == sizes[0]) {
            bound_size = sizes[0];
        }
    }
    fewest_bits += node_bits(tree, bitmap, tile, node);
    if node == Node::Divided || node == Node::Residual {
        if let Some((complex_tile, bits)) = best_complex_tile(bitmap, set_cells_before_each_word, tree, tile, bound_size, fewest_bits) {
            tree.set(tile, complex_tile);
            fewest_bits = bits;
        }
    }
    CountedSubtree { fewest_bits, bound_size }
}

/// `tile`'s cheapest complex tile, if one takes fewer than `to_beat`
/// bits: at `bound_size`, then raw, then as a cell list, the first of
/// the fewest bits kept.
fn best_complex_tile(bitmap: &Bitmap, set_cells_before_each_word: &SetCellsBeforeEachWord, tree: &Tree, tile: Tile, bound_size: Option<u8>, to_beat: u64) -> Option<(Node, u64)> {
    let mut best = (None, to_beat);
    let consider = |best: &mut (Option<Node>, u64), node: Node, bits: u64| {
        if bits < best.1 {
            *best = (Some(node), bits);
        }
    };
    if let Some(size) = bound_size {
        let node = Node::ComplexTile { size_offset: size - tile.level };
        consider(&mut best, node, node_bits(tree, bitmap, tile, node));
    }
    if raw_resolution_fits(tile.level) {
        let raw = Node::ComplexTile { size_offset: CELL_LEVEL - tile.level };
        let raw_bits = node_bits(tree, bitmap, tile, raw);
        consider(&mut best, raw, raw_bits);
        // A cell list's bits before its cells are a raw complex
        // tile's; its cells are counted only if the fewest they could
        // take beat everything so far.
        let header = raw_bits - cells_in_tile(tile.level) as u64;
        if header + cell_list_least_bits(tile.level, set_cells_before_each_word.in_tile(tile)) < best.1 {
            consider(&mut best, Node::CellList, node_bits(tree, bitmap, tile, Node::CellList));
        }
    }
    best.0.map(|node| (node, best.1))
}

/// What the rule places at the visited tile, if anything, and the
/// children it names, bit `i` for child `i`; `children_numbers` its
/// children's pattern numbers, for a tile coarser than 4x4, which
/// may name its children.
fn place(patterns: &Patterns, visit: Visit, children_numbers: Option<[u16; 4]>) -> Option<(Node, u8)> {
    if homogeneous_value_of(visit.pattern_number).is_some() {
        return Some((PLAIN_TILE, 0));
    }
    if let Some((far, direction)) = patterns.copy_source(visit.tile, visit.pattern_number) {
        return Some((Node::Copied { far, direction, names_children: false }, 0));
    }
    let numbers = children_numbers?;
    copy_naming_children(patterns, visit, numbers).or_else(|| flipping_divide(numbers, visit.bound_above))
}

/// The copy of the visited tile that copies the most of its
/// children, naming the rest, if it copies enough of them: near
/// before far, then in direction order, on a tie. A child is copied
/// when it holds the same cells as the same child of the copy's
/// source.
fn copy_naming_children(patterns: &Patterns, visit: Visit, numbers: [u16; 4]) -> Option<(Node, u8)> {
    let values = numbers.map(homogeneous_value_of);
    let child_level = visit.tile.level + 1;
    // Only a child whose pattern another tile holds can be copied.
    let can_match: [bool; 4] = std::array::from_fn(|index| patterns.repeats(child_level, numbers[index]));
    let adds_copied: [u32; 4] = std::array::from_fn(|index| (can_match[index] && values[index] != Some(visit.bound_above)) as u32);
    let adds_non_homogeneous: [u32; 4] = std::array::from_fn(|index| (can_match[index] && values[index].is_none()) as u32);
    let worth_it = |copied: u32, non_homogeneous: u32| copied >= MIN_COPIED_CHILDREN || non_homogeneous >= MIN_COPIED_NON_HOMOGENEOUS_CHILDREN;
    if !worth_it(adds_copied.iter().sum(), adds_non_homogeneous.iter().sum()) {
        return None;
    }
    let mut best: Option<(u32, Node, u8)> = None;
    for far in [false, true] {
        for direction in 0..DIRECTIONS {
            let Some(source) = visit.tile.offset_by(copy_offset(far, direction)) else { continue };
            let source_numbers = patterns.children_numbers(source);
            let (mut named, mut copied, mut non_homogeneous) = (0u8, 0, 0);
            for index in 0..numbers.len() {
                if can_match[index] && source_numbers[index] == numbers[index] {
                    copied += adds_copied[index];
                    non_homogeneous += adds_non_homogeneous[index];
                } else {
                    named |= 1 << index;
                }
            }
            if worth_it(copied, non_homogeneous) && best.is_none_or(|(most, ..)| copied > most) {
                best = Some((copied, Node::Copied { far, direction, names_children: true }, named));
            }
        }
    }
    best.map(|(_, node, named)| (node, named))
}

/// A flipping divide of a tile whose children's pattern numbers are
/// `numbers`, naming the children not homogeneous with the value not
/// bound above, if it binds at least [`MIN_FLIPPED_CHILDREN`].
fn flipping_divide(numbers: [u16; 4], bound_above: bool) -> Option<(Node, u8)> {
    let named = (0..numbers.len()).filter(|&index| homogeneous_value_of(numbers[index]) != Some(!bound_above)).fold(0u8, |named, index| named | 1 << index);
    (numbers.len() as u32 - named.count_ones() >= MIN_FLIPPED_CHILDREN).then_some((Node::FlippingDivide, named))
}

/// Whether each of the 4x4 `tile`'s 2x2s is homogeneous: its cells, one
/// run of the bitmap whose four quarters are its 2x2s.
fn all_2x2s_homogeneous(bitmap: &Bitmap, tile: Tile) -> bool {
    /// A 2x2's cells, all set.
    const QUARTER: u64 = (1 << cells_in_tile(FLOOR_LEVEL + 1)) - 1;
    let cells = bitmap.morton_run(tile.first_cell(), cells_in_tile(FLOOR_LEVEL));
    (0..4).all(|quarter| matches!(cells >> (quarter * cells_in_tile(FLOOR_LEVEL + 1)) & QUARTER, 0 | QUARTER))
}
