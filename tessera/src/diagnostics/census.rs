//! A tree's nodes, counted by kind and level.

use crate::tile::{Tile, CELL_LEVEL, FLOOR_LEVEL};
use crate::tree::{Node, Tree};
use std::collections::BTreeMap;

/// The node at `tile`'s kind, as `docs/tessera.md` names it.
pub fn kind(tree: &Tree, tile: Tile, node: Node) -> &'static str {
    match node {
        Node::ComplexTile { size_offset } if tile.level + size_offset == CELL_LEVEL => "raw",
        Node::ComplexTile { size_offset: 0 } => "tile",
        Node::ComplexTile { .. } => "complex tile",
        Node::FlippingDivide => "flipping divide",
        Node::Divided if tree.children(tile).contains(&Node::Absent) => "divide naming children",
        Node::Divided => "divide",
        Node::Copied { names_children: false, .. } => "copy",
        Node::Copied { names_children: true, .. } => "copy naming children",
        Node::Residual => "residual",
        Node::CellList => "cell list",
        Node::Absent => "absent",
    }
}

/// How many nodes of each kind a tree has at each level, whole bitmap
/// to the 4x4 floor.
pub type Census = BTreeMap<&'static str, [usize; FLOOR_LEVEL as usize + 1]>;

/// `tree`'s census, walked from the whole bitmap down: what lies under
/// a complex tile is not in the tree.
pub fn census(tree: &Tree) -> Census {
    let mut counts = Census::new();
    count(tree, Tile::WHOLE_BITMAP, &mut counts);
    counts
}

/// Counts `tile`'s node and every node under it.
fn count(tree: &Tree, tile: Tile, counts: &mut Census) {
    let node = tree.get(tile);
    counts.entry(kind(tree, tile, node)).or_default()[tile.level as usize] += 1;
    if node.has_children() {
        for child in tile.children() {
            if tree.get(child) != Node::Absent {
                count(tree, child, counts);
            }
        }
    }
}
