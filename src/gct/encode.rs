//! Encoding: the tree spelled out in bits, by the grammar
//! ([`crate::gct::grammar`]) -- the start level, the tree node by node
//! from there, each complex tile's payload after its body, then the
//! residual pass.

use crate::gct::grammar::bit_stream::BitStream;
use crate::gct::grammar::order::{payload_tiles, residual_cells};
use crate::gct::grammar::*;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{Tile, CELL_LEVEL};
use crate::Bitmap;

/// Spells out `tree` for `bitmap`.
pub fn write(tree: &Pyramid, bitmap: &Bitmap) -> BitStream {
    let mut out = BitStream::default();
    let start_level = tree.start_level();
    out.push_value(start_level as u64, START_LEVEL_WIDTH);
    for tile in Tile::all_of_level(start_level) {
        write_node(tree, bitmap, tile, &mut NestedResolutions::none(), &mut out);
    }
    for cell in residual_cells(tree) {
        out.push(cell.top_left_value(bitmap));
    }
    out
}

/// Writes `tile`'s node and everything under it.
fn write_node(tree: &Pyramid, bitmap: &Bitmap, tile: Tile, nested: &mut NestedResolutions, out: &mut BitStream) {
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
        Node::Copied { far, direction, masks } => {
            out.push_value(LEAF, LEAF_WIDTH);
            out.push_value(COPY, CODE_WIDTH);
            out.push_value(far as u64, FAR_WIDTH);
            out.push_value(direction as u64, DIRECTION_WIDTH);
            if copy_may_mask(tile.level) {
                out.push_value(if masks { MASKING } else { NO_MASKING }, MASK_PRESENT_WIDTH);
            }
            if masks {
                let masked: Vec<Tile> = tile.children().into_iter().filter(|&child| tree.node(child) != Node::Absent).collect();
                for child in tile.children() {
                    out.push_value(if masked.contains(&child) { MASKED } else { UNMASKED }, MASK_BIT_WIDTH);
                }
                for child in masked {
                    write_node(tree, bitmap, child, nested, out);
                }
            }
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

/// A complex tile's payload: the value of every tile of its resolution
/// unmasked in it.
fn write_payload(tree: &Pyramid, bitmap: &Bitmap, tile: Tile, nesting: u8, size_offset: u8, out: &mut BitStream) {
    for part in payload_tiles(tree, tile, nesting, size_offset) {
        out.push(part.top_left_value(bitmap));
    }
}
