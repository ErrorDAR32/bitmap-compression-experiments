//! Encoding: the tree spelled out in bits, by the grammar
//! ([`crate::gct::grammar`]) -- the start level, the tree node by node
//! from there, each complex tile's payload after its body, then the
//! residual pass.

use crate::gct::grammar::bit_stream::BitStream;
use crate::gct::grammar::order::PayloadWalk;
use crate::gct::grammar::point_list;
use crate::gct::grammar::*;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{cells_in_tile, Tile, CELL_LEVEL};
use crate::Bitmap;

/// Spells out `tree` for `bitmap` into `stream`, whatever it held before;
/// `payload_walk` is room for a payload's parts.
pub fn write(tree: &Pyramid, bitmap: &Bitmap, stream: &mut BitStream, payload_walk: &mut PayloadWalk) {
    stream.clear();
    let start_level = tree.start_level();
    stream.push_value(start_level as u64, START_LEVEL_WIDTH);
    for tile in Tile::all_of_level(start_level) {
        write_node(tree, bitmap, tile, &mut NestedResolutions::none(), stream, payload_walk);
    }
    for square in tree.residual_squares() {
        stream.push_value(bitmap.small_square(square.top_left_cell(), square.side_in_cells()), RESIDUAL_SQUARE_BITS);
    }
}

/// Writes `tile`'s node and everything under it.
fn write_node(tree: &Pyramid, bitmap: &Bitmap, tile: Tile, nested: &mut NestedResolutions, stream: &mut BitStream, payload_walk: &mut PayloadWalk) {
    let node = tree.node(tile);
    for nesting in nested.able_to_unmask(tile) {
        if node == (Node::Unmasked { nesting }) {
            stream.push_value(UNMASKED, MASK_BIT_WIDTH);
            return;
        }
        stream.push_value(MASKED, MASK_BIT_WIDTH);
    }

    if tile.level == CELL_LEVEL - 1 {
        match node {
            Node::ComplexTile { size_offset: 0, .. } => {
                stream.push_value(LEAF, LEAF_WIDTH);
                write_payload(tree, bitmap, tile, nested.next_nesting(), 0, stream, payload_walk);
            }
            Node::Residual => stream.push_value(RESIDUAL, LEAF_WIDTH),
            _ => unreachable!("the 2x2 floor is always a bound tile or residual"),
        }
        return;
    }

    match node {
        Node::Copied { far, direction, masks } => {
            stream.push_value(LEAF, LEAF_WIDTH);
            stream.push_value(COPY, CODE_WIDTH);
            stream.push_value(far as u64, FAR_WIDTH);
            stream.push_value(direction as u64, DIRECTION_WIDTH);
            if copy_may_mask(tile.level) {
                stream.push_value(if masks { MASKING } else { NO_MASKING }, MASK_PRESENT_WIDTH);
            }
            if masks {
                write_named_children(tree, bitmap, tile, nested, stream, payload_walk);
            }
        }
        Node::ComplexTile { size_offset: 0, masks: true } => {
            // A bind that masks: a divide that masks and flips the value
            // bound above.
            stream.push_value(SUBDIVIDE, LEAF_WIDTH);
            stream.push_value(MASKING, MASK_PRESENT_WIDTH);
            stream.push_value(BINDING_FLIPPED, FLIP_WIDTH);
            write_named_children(tree, bitmap, tile, nested, stream, payload_walk);
        }
        Node::ComplexTile { size_offset, masks } => {
            stream.push_value(LEAF, LEAF_WIDTH);
            stream.push_value(BIND, CODE_WIDTH);
            stream.push_value(size_offset as u64, resolution_width(tile.level));
            if complex_tile_may_mask(tile.level, size_offset) {
                stream.push_value(if masks { MASKING } else { NO_MASKING }, MASK_PRESENT_WIDTH);
            }
            if has_payload_mode(tile.level, size_offset, masks) {
                stream.push_value(PLAIN_PAYLOAD, PAYLOAD_MODE_WIDTH);
            }
            let nesting = nested.next_nesting();
            if masks {
                nested.while_nested(tile.level + size_offset, |inside| {
                    for child in tile.children() {
                        write_node(tree, bitmap, child, inside, stream, payload_walk);
                    }
                });
            }
            write_payload(tree, bitmap, tile, nesting, size_offset, stream, payload_walk);
        }
        Node::Subdivided => {
            stream.push_value(SUBDIVIDE, LEAF_WIDTH);
            if tree.divides_whole(tile) {
                if divide_may_mask(tile.level) {
                    stream.push_value(NO_MASKING, MASK_PRESENT_WIDTH);
                }
                for child in tile.children() {
                    write_node(tree, bitmap, child, nested, stream, payload_walk);
                }
            } else {
                stream.push_value(MASKING, MASK_PRESENT_WIDTH);
                stream.push_value(BINDING_KEPT, FLIP_WIDTH);
                write_named_children(tree, bitmap, tile, nested, stream, payload_walk);
            }
        }
        Node::PointList => {
            let size_offset = CELL_LEVEL - tile.level;
            stream.push_value(LEAF, LEAF_WIDTH);
            stream.push_value(BIND, CODE_WIDTH);
            stream.push_value(size_offset as u64, resolution_width(tile.level));
            stream.push_value(NO_MASKING, MASK_PRESENT_WIDTH);
            stream.push_value(POINT_LIST, PAYLOAD_MODE_WIDTH);
            point_list::write(bitmap, tile, stream);
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
    stream: &mut BitStream,
    payload_walk: &mut PayloadWalk,
) {
    let children = tile.children();
    let named = children.map(|child| tree.node(child) != Node::Absent);
    for is_named in named {
        stream.push_value(if is_named { MASKED } else { UNMASKED }, MASK_BIT_WIDTH);
    }
    for (child, is_named) in children.into_iter().zip(named) {
        if is_named {
            write_node(tree, bitmap, child, nested, stream, payload_walk);
        }
    }
}

/// A complex tile's payload: the value of every tile of its resolution
/// unmasked in it, a part at a time.
fn write_payload(tree: &Pyramid, bitmap: &Bitmap, tile: Tile, nesting: u8, size_offset: u8, stream: &mut BitStream, payload_walk: &mut PayloadWalk) {
    let resolution = tile.level + size_offset;
    for &part in payload_walk.parts(tree, tile, nesting) {
        write_part(bitmap, part, resolution, stream);
    }
}

/// One part of a payload: the value of each of its tiles of
/// `resolution`, in Morton order. At 1x1, those are its cells, as they
/// lie in the bitmap: written a word at a time.
fn write_part(bitmap: &Bitmap, part: Tile, resolution: u8, stream: &mut BitStream) {
    if resolution == CELL_LEVEL {
        let cells = cells_in_tile(part.level) as usize;
        for word in bitmap.square_words(part.top_left_cell(), part.side_in_cells()) {
            stream.push_value(word, cells.min(u64::BITS as usize) as u8);
        }
        return;
    }
    for tile in part.tiles_at_size_offset(resolution - part.level) {
        stream.push(tile.top_left_value(bitmap));
    }
}
