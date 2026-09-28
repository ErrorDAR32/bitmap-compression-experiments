//! The order of the two plain runs of value bits: a complex tile's
//! payload, and the residual pass. One walk each, used by both
//! directions, so writing and reading can never disagree.

use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{tiles_across, Tile, CELL_LEVEL, CHILDREN_ACROSS};

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

/// The residual cells, in reading order: every cell of every residual
/// 2x2.
///
/// The residual 2x2s are found by walking down the tree, only into
/// nodes that may hold nodes under them -- a divide, or anything that
/// masks -- then put in rows; each row of them gives its top cells left
/// to right, then its bottom ones.
pub fn residual_cells(tree: &Pyramid) -> impl Iterator<Item = Tile> {
    let floor = CELL_LEVEL - 1;
    let mut rows: Vec<Vec<u8>> = vec![Vec::new(); tiles_across(floor)];
    let mut pending = vec![Tile::whole_bitmap()];
    while let Some(at) = pending.pop() {
        match tree.node(at) {
            Node::Residual => rows[at.y as usize].push(at.x),
            Node::Subdivided | Node::ComplexTile { masks: true, .. } | Node::Copied { masks: true, .. } => {
                pending.extend(at.children())
            }
            _ => {}
        }
    }
    let mut cells = Vec::new();
    for (y, mut xs) in rows.into_iter().enumerate() {
        xs.sort_unstable();
        for row in 0..CHILDREN_ACROSS {
            for &x in &xs {
                for col in 0..CHILDREN_ACROSS {
                    cells.push(Tile { level: CELL_LEVEL, x: x * CHILDREN_ACROSS + col, y: y as u8 * CHILDREN_ACROSS + row });
                }
            }
        }
    }
    cells.into_iter()
}
