//! What a tree holds: tiles, complex tiles by how deeply nested they
//! are (`0` = enclosed by none), and, inside every complex tile's body,
//! its nodes -- related to it (unmasked, counted once per tile of its
//! resolution), or masked, by what they are instead. A masked node is
//! counted once, whatever its size, and belongs to the complex tile
//! whose body directly holds it.

use crate::cgt::pyramids::pyramid::Pyramid;
use crate::cgt::tile::Tile;
use crate::cgt::enclosing::Enclosing;
use crate::cgt::tree::node::{Node, Tree};

#[derive(Default, Clone, Debug)]
pub struct TreeStats {
    pub tiles: usize,
    pub complex_tiles_at_nesting: Vec<usize>,
    pub complex_tiles_masking: usize,
    pub unmasked: usize,
    pub masked_related_further_out: usize,
    pub masked_copied: usize,
    pub masked_tile: usize,
    pub masked_nested: usize,
    pub masked_hole: usize,
}

impl TreeStats {
    pub fn of(tree: &Pyramid) -> Self {
        let mut stats = Self::default();
        stats.count(tree, Tile::whole_bitmap(), None, &mut Enclosing::none());
        stats
    }

    pub fn masked(&self) -> usize {
        self.masked_related_further_out + self.masked_copied + self.masked_tile + self.masked_nested + self.masked_hole
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
        self.complex_tiles_masking += other.complex_tiles_masking;
        self.unmasked += other.unmasked;
        self.masked_related_further_out += other.masked_related_further_out;
        self.masked_copied += other.masked_copied;
        self.masked_tile += other.masked_tile;
        self.masked_nested += other.masked_nested;
        self.masked_hole += other.masked_hole;
    }

    /// `inside`: the nesting of the complex tile whose body directly
    /// holds `tile`, if any.
    fn count(&mut self, tree: &Pyramid, tile: Tile, inside: Option<usize>, enclosing: &mut Enclosing) {
        match tree.node(tile) {
            Node::Related { nesting } if inside == Some(nesting) => {
                self.unmasked += 1 << (2 * (enclosing.resolution(nesting) - tile.level));
            }
            Node::Related { .. } => self.masked_related_further_out += 1,
            Node::Copied { .. } if inside.is_some() => self.masked_copied += 1,
            Node::Hole if inside.is_some() => self.masked_hole += 1,
            Node::Complex { depth: 0, .. } => {
                self.tiles += 1;
                if inside.is_some() {
                    self.masked_tile += 1;
                }
            }
            Node::Complex { depth, masking } => {
                if inside.is_some() {
                    self.masked_nested += 1;
                }
                let nesting = enclosing.next_nesting();
                if self.complex_tiles_at_nesting.len() <= nesting {
                    self.complex_tiles_at_nesting.resize(nesting + 1, 0);
                }
                self.complex_tiles_at_nesting[nesting] += 1;
                if !masking {
                    self.unmasked += 1 << (2 * depth);
                    return;
                }
                self.complex_tiles_masking += 1;
                enclosing.within(tile.level + depth, |inner| {
                    for child in tile.children() {
                        self.count(tree, child, Some(nesting), inner);
                    }
                });
            }
            Node::Split => {
                for child in tile.children() {
                    self.count(tree, child, inside, enclosing);
                }
            }
            Node::Copied { .. } | Node::Hole | Node::None => {}
        }
    }
}
