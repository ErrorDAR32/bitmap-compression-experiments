//! The order of the two plain runs of value bits: a complex tile's
//! payload, and the residual pass. One walk each, used by both
//! directions, so writing and reading can never disagree.

use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{Tile, CELL_LEVEL};
use crate::Bitmap;

/// The tiles of its resolution unmasked in the complex tile at `tile`
/// (its own nesting `nesting`, size offset `size_offset`), in payload order: the
/// whole tile's resolution tiles when it masks nothing, otherwise every
/// node in its body unmasked in it, in body order -- including nodes
/// inside complex tiles nested in it.
pub fn payload_tiles(tree: &Pyramid, tile: Tile, nesting: u8, size_offset: u8) -> Vec<Tile> {
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
            Node::Subdivided | Node::ComplexTile { masks: true, .. } | Node::Copied { masks: true, .. } => {
                pending.extend(at.children().into_iter().rev())
            }
            _ => {}
        }
    }
    tiles
}

/// Every cell of every residual 2x2.
fn residual_cell_bitmap(tree: &Pyramid) -> Bitmap {
    let mut residual = Bitmap::new();
    for tile in Tile::all_of_level(CELL_LEVEL - 1) {
        if tree.node(tile) == Node::Residual {
            tile.set_in(&mut residual);
        }
    }
    residual
}

/// The residual cells, in reading order.
pub fn residual_cells(tree: &Pyramid) -> impl Iterator<Item = Tile> {
    let residual = residual_cell_bitmap(tree);
    Tile::all_cells().filter(move |cell| cell.top_left_value(&residual))
}
