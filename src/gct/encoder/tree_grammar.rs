//! The tree's grammar, both directions side by side: how each node of
//! the tree is spelled out in bits, and read back. The full grammar,
//! with its costs, is in `docs/gct.md`.

use super::bit_stream::{BitReader, BitStream};
use super::payload::{read_payload, write_payload};
use super::StreamContents;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{levels_to_cells, Tile, CELL_LEVEL};
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::Bitmap;

/// One mask bit per complex tile a node is nested in that could unmask
/// it, nearest first: unmasked in it, or masked.
const UNMASKED: u64 = 0;
const MASKED: u64 = 1;
const MASK_BIT_WIDTH: u8 = 1;

const LEAF: u64 = 1;
const SUBDIVIDE: u64 = 0;
const RESIDUAL: u64 = 0;
const LEAF_WIDTH: u8 = 1;

const COPY: u64 = 0;
const BIND: u64 = 1;
const CODE_WIDTH: u8 = 1;

const FAR_WIDTH: u8 = 1;
const DIRECTION_WIDTH: u8 = 2;

/// Whether a complex tile deeper than 1 masks anything at all. Skipped
/// at size offsets 0 and 1, which never mask.
const NO_MASKING: u64 = 0;
const MASKING: u64 = 1;
const MASK_PRESENT_WIDTH: u8 = 1;

/// How many bits name a complex tile's size offset at `level`: `0` (a tile)
/// up to a 2x2 resolution -- a 1x1 resolution never is one.
fn resolution_width(level: u8) -> u8 {
    let size_offsets = levels_to_cells(level);
    (u8::BITS - (size_offsets - 1).leading_zeros()) as u8
}

/// Writes `tile`'s node and everything under it.
pub fn write_node(tree: &Pyramid, bitmap: &Bitmap, tile: Tile, nested: &mut NestedResolutions, out: &mut BitStream) {
    let node = tree.node(tile);
    for nesting in nested.able_to_unmask(tile) {
        if node == (Node::Unmasked { nesting }) {
            out.push_value(UNMASKED, MASK_BIT_WIDTH);
            return;
        }
        out.push_value(MASKED, MASK_BIT_WIDTH);
    }

    if tile.level == CELL_LEVEL - 1 {
        match node {
            Node::ComplexTile { size_offset: 0, .. } => {
                out.push_value(LEAF, LEAF_WIDTH);
                write_payload(tree, bitmap, tile, nested.next_nesting(), 0, out);
            }
            Node::Residual => out.push_value(RESIDUAL, LEAF_WIDTH),
            _ => unreachable!("the 2x2 floor is always a bound tile or residual"),
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
        Node::ComplexTile { size_offset, masks } => {
            out.push_value(LEAF, LEAF_WIDTH);
            out.push_value(BIND, CODE_WIDTH);
            out.push_value(size_offset as u64, resolution_width(tile.level));
            if size_offset > 1 {
                out.push_value(if masks { MASKING } else { NO_MASKING }, MASK_PRESENT_WIDTH);
            }
            let nesting = nested.next_nesting();
            if masks {
                nested.while_nested(tile.level + size_offset, |inside| {
                    for child in tile.children() {
                        write_node(tree, bitmap, child, inside, out);
                    }
                });
            }
            write_payload(tree, bitmap, tile, nesting, size_offset, out);
        }
        Node::Subdivided => {
            out.push_value(SUBDIVIDE, LEAF_WIDTH);
            for child in tile.children() {
                write_node(tree, bitmap, child, nested, out);
            }
        }
        Node::Unmasked { .. } | Node::Residual | Node::Absent => unreachable!("{node:?} is never written here"),
    }
}

/// Reads `tile`'s node and everything under it, mirroring
/// [`write_node`].
pub fn read_node(reader: &mut BitReader, tile: Tile, nested: &mut NestedResolutions, read: &mut StreamContents) {
    let able_to_unmask: Vec<u8> = nested.able_to_unmask(tile).collect();
    for nesting in able_to_unmask {
        if reader.value(MASK_BIT_WIDTH) == UNMASKED {
            read.tree.set_node(tile, Node::Unmasked { nesting });
            return;
        }
    }

    let leaf = reader.value(LEAF_WIDTH) == LEAF;
    if tile.level == CELL_LEVEL - 1 {
        if !leaf {
            read.tree.set_node(tile, Node::Residual);
            return;
        }
        read.tree.set_node(tile, Node::ComplexTile { size_offset: 0, masks: false });
        read_payload(reader, tile, nested.next_nesting(), 0, read);
        return;
    }
    if !leaf {
        read.tree.set_node(tile, Node::Subdivided);
        for child in tile.children() {
            read_node(reader, child, nested, read);
        }
        return;
    }
    if reader.value(CODE_WIDTH) == COPY {
        let far = reader.value(FAR_WIDTH) != 0;
        let direction = reader.value(DIRECTION_WIDTH) as u8;
        read.tree.set_node(tile, Node::Copied { far, direction });
        return;
    }
    let size_offset = reader.value(resolution_width(tile.level)) as u8;
    let masks = size_offset > 1 && reader.value(MASK_PRESENT_WIDTH) == MASKING;
    read.tree.set_node(tile, Node::ComplexTile { size_offset, masks });
    let nesting = nested.next_nesting();
    if masks {
        nested.while_nested(tile.level + size_offset, |inside| {
            for child in tile.children() {
                read_node(reader, child, inside, read);
            }
        });
    }
    read_payload(reader, tile, nesting, size_offset, read);
}
