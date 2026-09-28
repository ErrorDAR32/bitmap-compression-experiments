//! The tree's grammar, both directions side by side: how each node of
//! the tree is spelled out in bits, and read back. The full grammar,
//! with its costs, is in `docs/gct.md`.

use super::bit_stream::{BitReader, BitStream};
use super::payload::{read_payload, write_payload};
use super::ReadBack;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{levels_to_cells, Tile, CELL_LEVEL};
use crate::gct::enclosing::Enclosing;
use crate::gct::tree::node::{Node, Tree};
use crate::Bitmap;

/// One bit per enclosing complex tile that could relate a node, nearest
/// first: related to it (unmasked) or not (masked).
const RELATED: u64 = 0;
const UNRELATED: u64 = 1;
const RELATION_WIDTH: usize = 1;

const LEAF: u64 = 1;
const SUBDIVIDE: u64 = 0;
const HOLE: u64 = 0;
const LEAF_WIDTH: usize = 1;

const COPY: u64 = 0;
const BIND: u64 = 1;
const CODE_WIDTH: usize = 1;

const FAR_WIDTH: usize = 1;
const DIRECTION_WIDTH: usize = 2;

/// Whether a complex tile deeper than 1 masks anything at all. Skipped
/// at depth 0 and 1, which never mask.
const NO_MASKING: u64 = 0;
const MASKING: u64 = 1;
const MASK_PRESENT_WIDTH: usize = 1;

/// How many bits name a complex tile's depth at `level`: `0` (a tile)
/// up to a 2x2 resolution -- a 1x1 resolution never is one.
fn resolution_width(level: usize) -> usize {
    let depths = levels_to_cells(level);
    (usize::BITS - (depths - 1).leading_zeros()) as usize
}

/// Writes `tile`'s node and everything under it.
pub fn write_node(tree: &Pyramid, bitmap: &Bitmap, tile: Tile, enclosing: &mut Enclosing, out: &mut BitStream) {
    let node = tree.node(tile);
    for nesting in enclosing.able_to_relate(tile) {
        if node == (Node::Related { nesting }) {
            out.push_value(RELATED, RELATION_WIDTH);
            return;
        }
        out.push_value(UNRELATED, RELATION_WIDTH);
    }

    if tile.level == CELL_LEVEL - 1 {
        match node {
            Node::Complex { depth: 0, .. } => {
                out.push_value(LEAF, LEAF_WIDTH);
                write_payload(tree, bitmap, tile, enclosing.next_nesting(), 0, out);
            }
            Node::Hole => out.push_value(HOLE, LEAF_WIDTH),
            _ => unreachable!("the 2x2 floor is always a tile or a hole"),
        }
        return;
    }

    match node {
        Node::Copied { far, direction } => {
            out.push_value(LEAF, LEAF_WIDTH);
            out.push_value(COPY, CODE_WIDTH);
            out.push_value(far as u64, FAR_WIDTH);
            out.push_value(direction as u64, DIRECTION_WIDTH);
        }
        Node::Complex { depth, masking } => {
            out.push_value(LEAF, LEAF_WIDTH);
            out.push_value(BIND, CODE_WIDTH);
            out.push_value(depth as u64, resolution_width(tile.level));
            if depth > 1 {
                out.push_value(if masking { MASKING } else { NO_MASKING }, MASK_PRESENT_WIDTH);
            }
            let nesting = enclosing.next_nesting();
            if masking {
                enclosing.within(tile.level + depth, |inside| {
                    for child in tile.children() {
                        write_node(tree, bitmap, child, inside, out);
                    }
                });
            }
            write_payload(tree, bitmap, tile, nesting, depth, out);
        }
        Node::Split => {
            out.push_value(SUBDIVIDE, LEAF_WIDTH);
            for child in tile.children() {
                write_node(tree, bitmap, child, enclosing, out);
            }
        }
        Node::Related { .. } | Node::Hole | Node::None => unreachable!("{node:?} is never written here"),
    }
}

/// Reads `tile`'s node and everything under it, mirroring
/// [`write_node`].
pub fn read_node(reader: &mut BitReader, tile: Tile, enclosing: &mut Enclosing, read: &mut ReadBack) {
    let able: Vec<usize> = enclosing.able_to_relate(tile).collect();
    for nesting in able {
        if reader.value(RELATION_WIDTH) == RELATED {
            read.tree.set_node(tile, Node::Related { nesting });
            return;
        }
    }

    let leaf = reader.value(LEAF_WIDTH) == LEAF;
    if tile.level == CELL_LEVEL - 1 {
        if !leaf {
            read.tree.set_node(tile, Node::Hole);
            return;
        }
        read.tree.set_node(tile, Node::Complex { depth: 0, masking: false });
        read_payload(reader, tile, enclosing.next_nesting(), 0, read);
        return;
    }
    if !leaf {
        read.tree.set_node(tile, Node::Split);
        for child in tile.children() {
            read_node(reader, child, enclosing, read);
        }
        return;
    }
    if reader.value(CODE_WIDTH) == COPY {
        let far = reader.value(FAR_WIDTH) != 0;
        let direction = reader.value(DIRECTION_WIDTH) as usize;
        read.tree.set_node(tile, Node::Copied { far, direction });
        return;
    }
    let depth = reader.value(resolution_width(tile.level)) as usize;
    let masking = depth > 1 && reader.value(MASK_PRESENT_WIDTH) == MASKING;
    read.tree.set_node(tile, Node::Complex { depth, masking });
    let nesting = enclosing.next_nesting();
    if masking {
        enclosing.within(tile.level + depth, |inside| {
            for child in tile.children() {
                read_node(reader, child, inside, read);
            }
        });
    }
    read_payload(reader, tile, nesting, depth, read);
}
