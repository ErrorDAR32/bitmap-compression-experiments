//! What a tree holds: tiles, complex tiles and the tiles of their
//! resolutions they say, masking nodes and cell lists.

use crate::pyramids::tree::{Node, Tree};
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
    /// Copies that mask some of their children.
    pub copies_that_mask: usize,
    /// Binds that mask some of their children.
    pub binds_that_mask: usize,
    /// Cell lists.
    pub cell_lists: usize,
}

impl TreeStats {
    /// The counts of one tree.
    pub fn of(tree: &Tree) -> Self {
        let mut stats = Self::default();
        stats.count(tree, Tile::whole_bitmap());
        stats
    }

    /// Adds `other`'s counts to these.
    pub fn add(&mut self, other: &TreeStats) {
        self.tiles += other.tiles;
        self.complex_tiles += other.complex_tiles;
        self.payload_values += other.payload_values;
        self.copies_that_mask += other.copies_that_mask;
        self.binds_that_mask += other.binds_that_mask;
        self.cell_lists += other.cell_lists;
    }

    /// Counts `tile`'s node and everything under it.
    fn count(&mut self, tree: &Tree, tile: Tile) {
        match tree.node(tile) {
            Node::Copied { masks: true, .. } => {
                self.copies_that_mask += 1;
                self.count_children(tree, tile);
            }
            Node::MaskingBind => {
                self.binds_that_mask += 1;
                self.count_children(tree, tile);
            }
            Node::Subdivided => self.count_children(tree, tile),
            Node::ComplexTile { size_offset: 0 } => self.tiles += 1,
            Node::ComplexTile { size_offset } => {
                self.complex_tiles += 1;
                self.payload_values += 1 << (2 * size_offset);
            }
            Node::CellList => self.cell_lists += 1,
            Node::Copied { masks: false, .. } | Node::Residual | Node::Absent => {}
        }
    }

    /// Counts every child of `tile` that is a node.
    fn count_children(&mut self, tree: &Tree, tile: Tile) {
        for child in tile.children() {
            self.count(tree, child);
        }
    }
}
