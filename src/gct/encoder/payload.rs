//! A complex tile's payload: one value bit for every tile of its
//! resolution unmasked in it, bound after its body. One walk,
//! [`payload_tiles`], says which tiles and in what order, for both
//! directions.

use super::bit_stream::{BitReader, BitStream};
use super::StreamContents;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::Tile;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::Bitmap;

/// The tiles of its resolution unmasked in the complex tile at `tile`
/// (its own nesting `nesting`, size offset `size_offset`), in payload order: the
/// whole tile's resolution tiles when it masks nothing, otherwise every
/// node in its body unmasked in it, in body order -- including nodes
/// inside complex tiles nested in it.
fn payload_tiles(tree: &Pyramid, tile: Tile, nesting: usize, size_offset: usize) -> Vec<Tile> {
    let resolution = tile.level + size_offset;
    let Node::ComplexTile { masks: true, .. } = tree.node(tile) else {
        return tile.tiles_at_size_offset(size_offset);
    };
    let mut tiles = Vec::new();
    let mut pending: Vec<Tile> = tile.children().into_iter().rev().collect();
    while let Some(at) = pending.pop() {
        match tree.node(at) {
            Node::Unmasked { nesting: unmasked_in } if unmasked_in == nesting => {
                tiles.extend(at.tiles_at_size_offset(resolution - at.level));
            }
            Node::Subdivided | Node::ComplexTile { masks: true, .. } => pending.extend(at.children().into_iter().rev()),
            _ => {}
        }
    }
    tiles
}

pub fn write_payload(tree: &Pyramid, bitmap: &Bitmap, tile: Tile, nesting: usize, size_offset: usize, out: &mut BitStream) {
    for part in payload_tiles(tree, tile, nesting, size_offset) {
        out.push(part.top_left_value(bitmap));
    }
}

pub fn read_payload(reader: &mut BitReader, tile: Tile, nesting: usize, size_offset: usize, read: &mut StreamContents) {
    for part in payload_tiles(&read.tree, tile, nesting, size_offset) {
        read.bind(part, reader.bit());
    }
}
