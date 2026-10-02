//! What a tree holds: tiles, complex tiles and the tiles of their
//! resolutions they say, nodes naming children, and cell lists.

use crate::tree::{Node, Tree};
use crate::tile::Tile;

/// The counts, over one tree or added up over many.
#[derive(Default, Clone, Debug)]
pub struct TreeStats {
    /// Whole binds that are nodes: complex tiles at size offset 0.
    pub tiles: usize,
    /// Complex tiles finer than size offset 0, cell lists aside.
    pub complex_tiles: usize,
    /// Tiles of a complex tile's resolution said in its payload: one bit
    /// each.
    pub payload_values: usize,
    /// Copies naming some of their children.
    pub copies_naming_children: usize,
    /// Flipping divides.
    pub flipping_divides: usize,
    /// Cell lists.
    pub cell_lists: usize,
}

impl TreeStats {
    /// The counts of one tree.
    pub fn of(tree: &Tree) -> Self {
        let mut stats = Self::default();
        stats.count(tree, Tile::WHOLE_BITMAP);
        stats
    }

    /// Adds `other`'s counts to these.
    pub fn add(&mut self, other: &TreeStats) {
        self.tiles += other.tiles;
        self.complex_tiles += other.complex_tiles;
        self.payload_values += other.payload_values;
        self.copies_naming_children += other.copies_naming_children;
        self.flipping_divides += other.flipping_divides;
        self.cell_lists += other.cell_lists;
    }

    /// Counts `tile`'s node and everything under it.
    fn count(&mut self, tree: &Tree, tile: Tile) {
        match tree.get(tile) {
            Node::Copied { names_children: true, .. } => {
                self.copies_naming_children += 1;
                self.count_children(tree, tile);
            }
            Node::FlippingDivide => {
                self.flipping_divides += 1;
                self.count_children(tree, tile);
            }
            Node::Divided => self.count_children(tree, tile),
            Node::ComplexTile { size_offset: 0 } => self.tiles += 1,
            Node::ComplexTile { size_offset } => {
                self.complex_tiles += 1;
                self.payload_values += 1 << (2 * size_offset);
            }
            Node::CellList => self.cell_lists += 1,
            Node::Copied { names_children: false, .. } | Node::Residual | Node::Absent => {}
        }
    }

    /// Counts every child of `tile` that is a node.
    fn count_children(&mut self, tree: &Tree, tile: Tile) {
        for child in tile.children() {
            self.count(tree, child);
        }
    }
}
