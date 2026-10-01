//! The grammar: what the tree holds, and every bit it is spelled in,
//! written and read side by side. Counting a node's bits is writing it
//! to a [`Counter`]: one spelling of every rule. `docs/tessera.md` has
//! the grammar in full, with its costs.
//!
//! A stream is its mode, then either the bitmap's [count
//! split](count_split), or the tree: the start level, then from every
//! tile of that level its node and everything under it, depth first --
//! each node's bits, its child mask and payload, then its children's
//! nodes -- then the [last pass](crate::last_pass).

pub mod cell_list;
pub mod count_split;

use crate::last_pass::LastPass;
use crate::tile::{cells_in_tile, tiles_in_level, Pyramid, Tile, CELL_LEVEL, CHILDREN, DIRECTIONS, FLOOR_LEVEL};
use crate::bit_stream::{BitReader, BitStream, Counter, Sink};
use bitmap::Bitmap;

/// What the tree holds at one tile.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Node {
    /// No node: the tile lies inside a coarser node's tile, or the value
    /// bound above says it.
    #[default]
    Absent,
    /// The tile's four children, each a node of its own -- or, the
    /// divide naming its children, the value bound above saying those
    /// that are not.
    Divided,
    /// A divide naming its children that flips the value bound above:
    /// every child not a node is bound to the other value. A bind with
    /// holes.
    FlippingDivide,
    /// A copy of a same-size tile; naming its children, it copies only
    /// those that are not nodes of their own.
    Copied {
        /// Whether it reads from the far offsets.
        far: bool,
        /// Which offset it reads from.
        direction: u8,
        /// Whether some of its children are nodes of their own.
        names_children: bool,
    },
    /// A value for every tile `size_offset` levels finer, each of them
    /// homogeneous; at size offset 0, the tile bound whole.
    ComplexTile {
        /// How many levels finer than the tile its resolution is.
        size_offset: u8,
    },
    /// A complex tile of 1x1 resolution saying its set cells as a cell
    /// list.
    CellList,
    /// A 4x4 whose cells are left to the last pass.
    Residual,
}

impl Node {
    /// Whether its children's nodes follow it.
    fn has_children(self) -> bool {
        matches!(self, Node::Divided | Node::FlippingDivide | Node::Copied { names_children: true, .. })
    }
}

/// The tree: a node a tile, down to the 4x4 floor.
pub type Tree = Pyramid<Node, FLOOR_LEVEL>;

/// The value bound at the top of the bitmap, before any flipping
/// divide: clear.
pub const BOUND_AT_THE_TOP: bool = false;

/// Bits in every one-bit choice below.
pub const FLAG_WIDTH: u8 = 1;
/// The stream's mode: the tree follows...
pub const TREE_STREAM: u64 = 0;
/// ...or the count split.
pub const COUNT_SPLIT_STREAM: u64 = 1;
/// A node's first bit: a leaf, a copy or a bind...
const LEAF: u64 = 1;
/// ...or a divide -- at the 4x4 floor, a residual block.
const DIVIDE: u64 = 0;
/// After a leaf's first bit: a copy...
const COPY: u64 = 0;
/// ...or a bind, a complex tile.
const BIND: u64 = 1;
/// Whether a copy or a divide names its children: its child mask
/// follows. Every divide is 8x8 or coarser -- a 4x4 is a leaf or a
/// residual block -- and so says it; a copy, down to 8x8 only: a 4x4's
/// children are finer than the tree.
const NAMES_CHILDREN: u64 = 1;
/// A divide naming its children flips the value bound above...
const BINDING_FLIPPED: u64 = 1;
/// A child mask's bit, a child in reading order: a node of its own,
/// following the parent's bits, or said by the parent.
const CHILD_IS_NODE: u64 = 1;
/// A bind's size offset starts with this for a plain tile, nothing
/// more, and else its size offset follows, in truncated binary over the
/// size offsets its level allows, finest first: the finest, 1x1 above
/// all, are what complex tiles are mostly made at.
const PLAIN_TILE: u64 = 0;
/// A complex tile of 1x1 resolution says its cells as a cell list,
/// else raw.
const CELL_LIST: u64 = 1;
/// Bits naming a copy's direction.
const DIRECTION_WIDTH: u8 = DIRECTIONS.trailing_zeros() as u8;
/// Bits naming the start level, the whole bitmap to the 4x4 floor.
pub const START_LEVEL_WIDTH: u8 = (u8::BITS - FLOOR_LEVEL.leading_zeros()) as u8;
/// The most bits one node takes, its payload aside: a copy naming its
/// children -- leaf, copy, far, direction, names children, a bit a
/// child.
pub const MOST_NODE_BITS: usize = (4 * FLAG_WIDTH + DIRECTION_WIDTH + CHILDREN * FLAG_WIDTH) as usize;

/// Whether a copy at `level` says whether it names its children.
const fn may_name_children(level: u8) -> bool {
    level < FLOOR_LEVEL
}

/// The largest size offset a complex tile at `level` can have: down to
/// 2x2, or to 1x1 -- every cell raw, the escape for what nothing else
/// compresses -- where the truncated binary code has room for it.
const fn most_size_offset(level: u8) -> u8 {
    let to_cells = CELL_LEVEL - level;
    let width = u8::BITS - (to_cells - 1).leading_zeros();
    if (to_cells as u32) < 1 << width { to_cells } else { to_cells - 1 }
}

/// Whether a 1x1 resolution -- and so a cell list -- can be named at
/// `level`.
pub const fn raw_resolution_fits(level: u8) -> bool {
    most_size_offset(level) == CELL_LEVEL - level
}

/// Writes one flag.
#[inline]
fn push_flag(sink: &mut impl Sink, value: u64) {
    sink.push_value(value, FLAG_WIDTH);
}

/// Reads one flag.
#[inline]
fn read_flag(reader: &mut BitReader) -> u64 {
    reader.value(FLAG_WIDTH)
}

/// The bits `node` takes at `tile` itself: [`write_node`], counted.
pub fn node_bits(tree: &Tree, bitmap: &Bitmap, tile: Tile, node: Node) -> u64 {
    let mut counter = Counter::default();
    write_node(&mut counter, tree, bitmap, tile, node);
    counter.0
}

/// Writes `node`, at `tile`, whose children's nodes are in `tree`: its
/// own bits, child mask and payload -- everything but its children's
/// nodes.
pub fn write_node(sink: &mut impl Sink, tree: &Tree, bitmap: &Bitmap, tile: Tile, node: Node) {
    match node {
        Node::Divided | Node::FlippingDivide => {
            push_flag(sink, DIVIDE);
            let names_children = node == Node::FlippingDivide || tree.children(tile).contains(&Node::Absent);
            push_flag(sink, names_children as u64);
            if names_children {
                push_flag(sink, (node == Node::FlippingDivide) as u64);
                write_child_mask(sink, tree, tile);
            }
        }
        Node::Copied { far, direction, names_children } => {
            push_flag(sink, LEAF);
            push_flag(sink, COPY);
            push_flag(sink, far as u64);
            sink.push_value(direction as u64, DIRECTION_WIDTH);
            if may_name_children(tile.level) {
                push_flag(sink, names_children as u64);
            }
            if names_children {
                write_child_mask(sink, tree, tile);
            }
        }
        Node::ComplexTile { size_offset } => {
            write_complex_tile_header(sink, tile.level, size_offset, false);
            write_payload(sink, bitmap, tile, size_offset);
        }
        Node::CellList => {
            write_complex_tile_header(sink, tile.level, CELL_LEVEL - tile.level, true);
            cell_list::write(sink, bitmap, tile);
        }
        Node::Residual => push_flag(sink, DIVIDE),
        Node::Absent => unreachable!("an absent node is never written"),
    }
}

/// A child mask: a bit a child, whether it is a node of its own.
fn write_child_mask(sink: &mut impl Sink, tree: &Tree, tile: Tile) {
    for child in tree.children(tile) {
        push_flag(sink, (child != Node::Absent) as u64);
    }
}

/// Reads a child mask.
fn read_child_mask(reader: &mut BitReader) -> [bool; 4] {
    [(); 4].map(|_| read_flag(reader) == CHILD_IS_NODE)
}

/// A complex tile's bits before its payload, at `level`, of
/// `size_offset`: leaf, bind, its size offset and, at 1x1 resolution,
/// whether it is a cell list.
fn write_complex_tile_header(sink: &mut impl Sink, level: u8, size_offset: u8, cell_list: bool) {
    push_flag(sink, LEAF);
    push_flag(sink, BIND);
    push_flag(sink, (size_offset != 0) as u64);
    if size_offset != 0 {
        sink.push_truncated_binary((most_size_offset(level) - size_offset) as u64, most_size_offset(level) as u64);
    }
    if level + size_offset == CELL_LEVEL {
        push_flag(sink, cell_list as u64);
    }
}

/// A complex tile's payload: the value of each of `tile`'s tiles
/// `size_offset` levels finer, in Morton order -- at 1x1, its cells as
/// they lie in the bitmap, a word at a time.
fn write_payload(sink: &mut impl Sink, bitmap: &Bitmap, tile: Tile, size_offset: u8) {
    if let Some(bits) = sink.counted() {
        *bits += tiles_in_level(size_offset) as u64;
        return;
    }
    if tile.level + size_offset == CELL_LEVEL {
        let width = cells_in_tile(tile.level).min(u64::BITS as usize) as u8;
        for word in bitmap.square_words(tile.top_left_cell(), tile.side_in_cells()) {
            sink.push_value(word, width);
        }
        return;
    }
    for resolution_tile in tile.tiles_under(size_offset) {
        sink.push(resolution_tile.top_left_value(bitmap));
    }
}

/// Reads what [`write_payload`] wrote into `cells`.
fn read_payload(reader: &mut BitReader, cells: &mut Bitmap, tile: Tile, size_offset: u8) {
    let (corner, side) = (tile.top_left_cell(), tile.side_in_cells());
    if tile.level + size_offset != CELL_LEVEL {
        for resolution_tile in tile.tiles_under(size_offset) {
            if reader.bit() {
                cells.set_square(resolution_tile.top_left_cell(), resolution_tile.side_in_cells());
            }
        }
    } else if side * side < u64::BITS as usize {
        cells.set_in_small_square(corner, side, reader.value((side * side) as u8));
    } else {
        for word in cells.square_words_mut(corner, side) {
            *word |= reader.value(u64::BITS as u8);
        }
    }
}

/// The level the tree starts at: that of its coarsest tile that is not
/// a divide with every child a node. Every coarser tile is one -- the
/// trunk, which the stream never spells out.
pub fn start_level(tree: &Tree) -> u8 {
    (0..=FLOOR_LEVEL)
        .find(|&level| {
            Tile::all_of_level(level).any(|tile| tree.get(tile) != Node::Divided || tree.children(tile).contains(&Node::Absent))
        })
        .expect("the 4x4 floor never divides")
}

/// Writes `tree`, which starts at `start_level`, for `bitmap`: the start
/// level, then every node from there, each copy and residual block
/// noted in `last_pass`.
pub fn write_tree(stream: &mut BitStream, tree: &Tree, bitmap: &Bitmap, last_pass: &mut LastPass, start_level: u8) {
    stream.push_value(start_level as u64, START_LEVEL_WIDTH);
    for tile in Tile::all_of_level(start_level) {
        write_subtree(stream, tree, bitmap, last_pass, tile);
    }
}

/// Writes `tile`'s node and everything under it.
fn write_subtree(stream: &mut BitStream, tree: &Tree, bitmap: &Bitmap, last_pass: &mut LastPass, tile: Tile) {
    let node = tree.get(tile);
    write_node(stream, tree, bitmap, tile, node);
    match node {
        Node::Residual => last_pass.note_residual(tile),
        Node::Copied { far, direction, names_children: false } => last_pass.note_copy(tile, tile, far, direction),
        _ => {}
    }
    if node.has_children() {
        for (child, child_node) in tile.children().into_iter().zip(tree.children(tile)) {
            match (child_node, node) {
                (Node::Absent, Node::Copied { far, direction, .. }) => last_pass.note_copy(tile, child, far, direction),
                (Node::Absent, _) => {}
                _ => write_subtree(stream, tree, bitmap, last_pass, child),
            }
        }
    }
}

/// Reads back what [`write_tree`] wrote into `cells`, which start
/// clear: every cell the tree says, each copy and residual block noted
/// in `last_pass`.
pub fn read_tree(reader: &mut BitReader, cells: &mut Bitmap, last_pass: &mut LastPass) {
    let start_level = reader.value(START_LEVEL_WIDTH) as u8;
    for tile in Tile::all_of_level(start_level) {
        read_subtree(reader, cells, last_pass, tile, BOUND_AT_THE_TOP);
    }
}

/// Reads `tile`'s node and everything under it, `bound_above` the value
/// bound above it.
fn read_subtree(reader: &mut BitReader, cells: &mut Bitmap, last_pass: &mut LastPass, tile: Tile, bound_above: bool) {
    if read_flag(reader) == DIVIDE {
        if tile.level == FLOOR_LEVEL {
            last_pass.note_residual(tile);
        } else if read_flag(reader) != NAMES_CHILDREN {
            for child in tile.children() {
                read_subtree(reader, cells, last_pass, child, bound_above);
            }
        } else {
            let bound_inside = bound_above != (read_flag(reader) == BINDING_FLIPPED);
            for (child, is_node) in tile.children().into_iter().zip(read_child_mask(reader)) {
                if is_node {
                    read_subtree(reader, cells, last_pass, child, bound_inside);
                } else if bound_inside {
                    cells.set_square(child.top_left_cell(), child.side_in_cells());
                }
            }
        }
    } else if read_flag(reader) == COPY {
        let far = reader.bit();
        let direction = reader.value(DIRECTION_WIDTH) as u8;
        if !may_name_children(tile.level) || read_flag(reader) != NAMES_CHILDREN {
            last_pass.note_copy(tile, tile, far, direction);
            return;
        }
        for (child, is_node) in tile.children().into_iter().zip(read_child_mask(reader)) {
            if is_node {
                read_subtree(reader, cells, last_pass, child, bound_above);
            } else {
                last_pass.note_copy(tile, child, far, direction);
            }
        }
    } else {
        let size_offset =
            if read_flag(reader) == PLAIN_TILE { 0 } else { most_size_offset(tile.level) - reader.truncated_binary(most_size_offset(tile.level) as u64) as u8 };
        if tile.level + size_offset == CELL_LEVEL && read_flag(reader) == CELL_LIST {
            cell_list::read(reader, tile, cells);
        } else {
            read_payload(reader, cells, tile, size_offset);
        }
    }
}
