//! What a tree holds: tiles, complex tiles by how deeply nested they
//! are (`0` = nested in none), and, inside every complex tile's body,
//! its nodes -- unmasked in it ( counted once per tile of its
//! resolution), or masked, by what they are instead. A masked node is
//! counted once, whatever its size, and belongs to the complex tile
//! whose body directly holds it.

use bitmap::gct::pyramids::pyramid::Pyramid;
use bitmap::gct::tile::Tile;
use bitmap::gct::nested_resolutions::NestedResolutions;
use bitmap::gct::pyramids::tree::{Node, Tree};

#[derive(Default, Clone, Debug)]
pub struct TreeStats {
    pub tiles: usize,
    pub complex_tiles_at_nesting: Vec<usize>,
    pub complex_tiles_that_mask: usize,
    pub copies_that_mask: usize,
    pub unmasked: usize,
    pub unmasked_in_outer: usize,
    pub masked_copied: usize,
    pub masked_tile: usize,
    pub masked_nested: usize,
    pub masked_residual: usize,
}

impl TreeStats {
    pub fn of(tree: &Pyramid) -> Self {
        let mut stats = Self::default();
        stats.count(tree, Tile::whole_bitmap(), None, &mut NestedResolutions::none());
        stats
    }

    pub fn masked(&self) -> usize {
        self.unmasked_in_outer + self.masked_copied + self.masked_tile + self.masked_nested + self.masked_residual
    }

    pub fn complex_tiles(&self) -> usize {
        self.complex_tiles_at_nesting.iter().sum()
    }

    pub fn add(&mut self, other: &TreeStats) {
        if self.complex_tiles_at_nesting.len() < other.complex_tiles_at_nesting.len() {
            self.complex_tiles_at_nesting.resize(other.complex_tiles_at_nesting.len(), 0);
        }
        for (total, added) in self.complex_tiles_at_nesting.iter_mut().zip(&other.complex_tiles_at_nesting) {
            *total += added;
        }
        self.tiles += other.tiles;
        self.complex_tiles_that_mask += other.complex_tiles_that_mask;
        self.copies_that_mask += other.copies_that_mask;
        self.unmasked += other.unmasked;
        self.unmasked_in_outer += other.unmasked_in_outer;
        self.masked_copied += other.masked_copied;
        self.masked_tile += other.masked_tile;
        self.masked_nested += other.masked_nested;
        self.masked_residual += other.masked_residual;
    }

    /// `inside`: the nesting of the complex tile whose body directly
    /// holds `tile`, if any.
    fn count(&mut self, tree: &Pyramid, tile: Tile, inside: Option<u8>, nested: &mut NestedResolutions) {
        match tree.node(tile) {
            Node::Unmasked { nesting } if inside == Some(nesting) => {
                self.unmasked += 1 << (2 * (nested.resolution(nesting) - tile.level));
            }
            Node::Unmasked { .. } => self.unmasked_in_outer += 1,
            Node::Copied { masks, .. } => {
                if inside.is_some() {
                    self.masked_copied += 1;
                }
                if masks {
                    self.copies_that_mask += 1;
                    for child in tile.children() {
                        self.count(tree, child, inside, nested);
                    }
                }
            }
            Node::Residual if inside.is_some() => self.masked_residual += 1,
            Node::ComplexTile { size_offset: 0, .. } => {
                self.tiles += 1;
                if inside.is_some() {
                    self.masked_tile += 1;
                }
            }
            Node::ComplexTile { size_offset, masks } => {
                if inside.is_some() {
                    self.masked_nested += 1;
                }
                let nesting = nested.next_nesting();
                let at = nesting as usize;
                if self.complex_tiles_at_nesting.len() <= at {
                    self.complex_tiles_at_nesting.resize(at + 1, 0);
                }
                self.complex_tiles_at_nesting[at] += 1;
                if !masks {
                    self.unmasked += 1 << (2 * size_offset);
                    return;
                }
                self.complex_tiles_that_mask += 1;
                nested.while_nested(tile.level + size_offset, |inner| {
                    for child in tile.children() {
                        self.count(tree, child, Some(nesting), inner);
                    }
                });
            }
            Node::Subdivided => {
                for child in tile.children() {
                    self.count(tree, child, inside, nested);
                }
            }
            Node::Residual | Node::Absent => {}
        }
    }
}
