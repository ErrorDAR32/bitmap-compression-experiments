//! Encoding: the tree spelled out in bits, by the grammar
//! ([`crate::gct::grammar`]) -- the start level, the tree node by node
//! from there, each complex tile's payload after its body, then the
//! residual pass.

use crate::gct::grammar::bit_stream::BitStream;
use crate::gct::grammar::order::Runs;
use crate::gct::grammar::point_list;
use crate::gct::grammar::*;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{Tile, CELL_LEVEL};
use crate::Bitmap;

/// Spells out `tree` for `bitmap` into `out`, whatever it held before;
/// `runs` is room for the runs' tiles.
pub fn write(tree: &Pyramid, bitmap: &Bitmap, out: &mut BitStream, runs: &mut Runs) {
    out.clear();
    let start_level = tree.start_level();
    out.push_value(start_level as u64, START_LEVEL_WIDTH);
    for tile in Tile::all_of_level(start_level) {
        write_node(tree, bitmap, tile, &mut NestedResolutions::none(), out, runs);
    }
    for cell in runs.residual_cells(tree) {
        out.push(cell.top_left_value(bitmap));
    }
}

/// Writes `tile`'s node and everything under it.
fn write_node(tree: &Pyramid, bitmap: &Bitmap, tile: Tile, nested: &mut NestedResolutions, out: &mut BitStream, runs: &mut Runs) {
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
                write_payload(tree, bitmap, tile, nested.next_nesting(), 0, out, runs);
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
                write_named_children(tree, bitmap, tile, nested, out, runs);
            }
        }
        Node::ComplexTile { size_offset: 0, masks: true } => {
            // A bind that masks: a divide that masks and flips the value
            // bound above.
            out.push_value(SUBDIVIDE, LEAF_WIDTH);
            out.push_value(MASKING, MASK_PRESENT_WIDTH);
            out.push_value(BINDING_FLIPPED, FLIP_WIDTH);
            write_named_children(tree, bitmap, tile, nested, out, runs);
        }
        Node::ComplexTile { size_offset, masks } => {
            out.push_value(LEAF, LEAF_WIDTH);
            out.push_value(BIND, CODE_WIDTH);
            out.push_value(size_offset as u64, resolution_width(tile.level));
            if complex_tile_may_mask(tile.level, size_offset) {
                out.push_value(if masks { MASKING } else { NO_MASKING }, MASK_PRESENT_WIDTH);
            }
            if has_payload_mode(tile.level, size_offset, masks) {
                out.push_value(PLAIN_PAYLOAD, PAYLOAD_MODE_WIDTH);
            }
            let nesting = nested.next_nesting();
            if masks {
                nested.while_nested(tile.level + size_offset, |inside| {
                    for child in tile.children() {
                        write_node(tree, bitmap, child, inside, out, runs);
                    }
                });
            }
            write_payload(tree, bitmap, tile, nesting, size_offset, out, runs);
        }
        Node::Subdivided => {
            out.push_value(SUBDIVIDE, LEAF_WIDTH);
            if tree.divides_whole(tile) {
                if divide_may_mask(tile.level) {
                    out.push_value(NO_MASKING, MASK_PRESENT_WIDTH);
                }
                for child in tile.children() {
                    write_node(tree, bitmap, child, nested, out, runs);
                }
            } else {
                out.push_value(MASKING, MASK_PRESENT_WIDTH);
                out.push_value(BINDING_KEPT, FLIP_WIDTH);
                write_named_children(tree, bitmap, tile, nested, out, runs);
            }
        }
        Node::PointList => {
            let size_offset = CELL_LEVEL - tile.level;
            out.push_value(LEAF, LEAF_WIDTH);
            out.push_value(BIND, CODE_WIDTH);
            out.push_value(size_offset as u64, resolution_width(tile.level));
            out.push_value(NO_MASKING, MASK_PRESENT_WIDTH);
            out.push_value(POINT_LIST, PAYLOAD_MODE_WIDTH);
            point_list::write(bitmap, tile, out);
        }
        Node::Unmasked { .. } | Node::Residual | Node::Absent => unreachable!("{node:?} is never written here"),
    }
}

/// A masking node's child mask -- each child a node of its own, or said
/// by the node: left to the binding above, or copied -- then the
/// children that are nodes.
fn write_named_children(
    tree: &Pyramid,
    bitmap: &Bitmap,
    tile: Tile,
    nested: &mut NestedResolutions,
    out: &mut BitStream,
    runs: &mut Runs,
) {
    let children = tile.children();
    let named = children.map(|child| tree.node(child) != Node::Absent);
    for is_named in named {
        out.push_value(if is_named { MASKED } else { UNMASKED }, MASK_BIT_WIDTH);
    }
    for (child, is_named) in children.into_iter().zip(named) {
        if is_named {
            write_node(tree, bitmap, child, nested, out, runs);
        }
    }
}

/// A complex tile's payload: the value of every tile of its resolution
/// unmasked in it.
fn write_payload(tree: &Pyramid, bitmap: &Bitmap, tile: Tile, nesting: u8, size_offset: u8, out: &mut BitStream, runs: &mut Runs) {
    for part in runs.payload(tree, tile, nesting, size_offset) {
        out.push(part.top_left_value(bitmap));
    }
}
