//! The order of the two plain runs of value bits: a complex tile's
//! payload, and the residual pass. One walk each, used by both
//! directions, so writing and reading can never disagree.

use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{Tile, CELL_LEVEL, CHILDREN_ACROSS};

/// Room for the runs' tiles, kept from one run to the next so walking
/// them allocates nothing once warm.
#[derive(Default)]
pub struct Runs {
    /// The tiles of the last run walked.
    tiles: Vec<Tile>,
    /// Tiles still to visit, or residual 2x2s found, while walking.
    pending: Vec<Tile>,
}

impl Runs {
    /// The tiles of its resolution unmasked in the complex tile at `tile`
    /// (its own nesting `nesting`, size offset `size_offset`), in payload
    /// order: the whole tile's resolution tiles when it masks nothing,
    /// otherwise every node in its body unmasked in it, in body order --
    /// including nodes inside complex tiles nested in it.
    pub fn payload(&mut self, tree: &Pyramid, tile: Tile, nesting: u8, size_offset: u8) -> &[Tile] {
        self.tiles.clear();
        let resolution = tile.level + size_offset;
        let Node::ComplexTile { masks: true, .. } = tree.node(tile) else {
            self.tiles.extend(tile.tiles_at_size_offset(size_offset));
            return &self.tiles;
        };
        self.pending.clear();
        self.pending.extend(tile.children().into_iter().rev());
        while let Some(at) = self.pending.pop() {
            match tree.node(at) {
                Node::Unmasked { nesting: unmasked_in } if unmasked_in == nesting => {
                    self.tiles.extend(at.tiles_at_size_offset(resolution - at.level));
                }
                Node::Subdivided | Node::ComplexTile { masks: true, .. } | Node::Copied { masks: true, .. } => {
                    self.pending.extend(at.children().into_iter().rev())
                }
                _ => {}
            }
        }
        &self.tiles
    }

    /// The residual cells, in reading order: every cell of every residual
    /// 2x2.
    ///
    /// The residual 2x2s are found by walking down the tree, only into
    /// nodes that may hold nodes under them -- a divide, or anything that
    /// masks -- then put in reading order; each row of them gives its top
    /// cells left to right, then its bottom ones.
    pub fn residual_cells(&mut self, tree: &Pyramid) -> &[Tile] {
        let mut squares = std::mem::take(&mut self.pending);
        squares.clear();
        self.tiles.clear();
        self.tiles.push(Tile::whole_bitmap());
        while let Some(at) = self.tiles.pop() {
            match tree.node(at) {
                Node::Residual => squares.push(at),
                Node::Subdivided | Node::ComplexTile { masks: true, .. } | Node::Copied { masks: true, .. } => {
                    self.tiles.extend(at.children())
                }
                _ => {}
            }
        }
        squares.sort_unstable_by_key(|square| (square.y, square.x));
        for row_of_squares in squares.chunk_by(|a, b| a.y == b.y) {
            for row in 0..CHILDREN_ACROSS {
                for square in row_of_squares {
                    for col in 0..CHILDREN_ACROSS {
                        self.tiles.push(Tile {
                            level: CELL_LEVEL,
                            x: square.x * CHILDREN_ACROSS + col,
                            y: square.y * CHILDREN_ACROSS + row,
                        });
                    }
                }
            }
        }
        self.pending = squares;
        &self.tiles
    }
}
