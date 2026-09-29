//! A tree's nodes, counted by kind and level.

use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{Tile, CELL_LEVEL, FLOOR_LEVEL};
use std::collections::BTreeMap;

/// The node at `tile`'s kind, named as the grammar spells it.
pub fn kind(tree: &Tree, tile: Tile, node: Node) -> &'static str {
    match node {
        Node::ComplexTile { size_offset } if tile.level + size_offset == CELL_LEVEL => "raw",
        Node::ComplexTile { size_offset: 0 } => "tile",
        Node::ComplexTile { .. } => "complex tile",
        Node::MaskingBind => "masking bind",
        Node::Subdivided if tree.divides_whole(tile) => "divide",
        Node::Subdivided => "masking divide",
        Node::Copied { masks: false, .. } => "copy",
        Node::Copied { masks: true, .. } => "masking copy",
        Node::Residual => "residual",
        Node::CellList => "cell list",
        Node::Absent => "absent",
    }
}

/// How many nodes of each kind a tree has at each level, whole bitmap
/// to the 4x4 floor; absent ones not counted.
pub type Census = BTreeMap<&'static str, [usize; FLOOR_LEVEL as usize + 1]>;

/// `tree`'s census.
pub fn census(tree: &Tree) -> Census {
    let mut counts = Census::new();
    for level in 0..=FLOOR_LEVEL {
        for tile in Tile::all_of_level(level) {
            let node = tree.node(tile);
            if node != Node::Absent {
                counts.entry(kind(tree, tile, node)).or_default()[level as usize] += 1;
            }
        }
    }
    counts
}
