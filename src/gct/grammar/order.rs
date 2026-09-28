//! The order of the two plain runs of value bits: a complex tile's
//! payload, and the residual pass. One walk each, used by both
//! directions, so writing and reading can never disagree.

use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::fixed_list::FixedList;
use crate::gct::tile::{Tile, CELLS, CELL_LEVEL};

/// The most tiles a walk down the tree waits on at once: three siblings
/// a level, and a last level's four.
const MOST_WAITING: usize = 3 * CELL_LEVEL as usize + 4;

/// Room for the runs' tiles, allocated once at the most any run takes.
#[derive(Default)]
pub struct Runs {
    /// The tiles of the last run walked: never more than there are
    /// cells, each a run's tile covering at least one.
    tiles: FixedList<Tile, CELLS>,
    /// Tiles still to visit while walking down the tree.
    waiting: FixedList<Tile, MOST_WAITING>,
}

impl Runs {
    /// The tiles of its resolution unmasked in the complex tile at `tile`
    /// (its own nesting `nesting`, size offset `size_offset`), in payload
    /// order: the whole tile's resolution tiles when it masks nothing,
    /// otherwise every node in its body unmasked in it, in body order --
    /// including nodes inside complex tiles nested in it -- each node's
    /// resolution tiles in Morton order.
    pub fn payload(&mut self, tree: &Pyramid, tile: Tile, nesting: u8, size_offset: u8) -> &[Tile] {
        self.tiles.clear();
        let resolution = tile.level + size_offset;
        let Node::ComplexTile { masks: true, .. } = tree.node(tile) else {
            self.tiles.extend(tile.tiles_at_size_offset(size_offset));
            return &self.tiles;
        };
        self.waiting.clear();
        self.waiting.extend(tile.children().into_iter().rev());
        while let Some(at) = self.waiting.pop() {
            match tree.node(at) {
                Node::Unmasked { nesting: unmasked_in } if unmasked_in == nesting => {
                    self.tiles.extend(at.tiles_at_size_offset(resolution - at.level));
                }
                Node::Subdivided | Node::ComplexTile { masks: true, .. } | Node::Copied { masks: true, .. } => {
                    self.waiting.extend(at.children().into_iter().rev())
                }
                _ => {}
            }
        }
        &self.tiles
    }

    /// The residual 2x2s, in Morton order, found by walking down the tree
    /// in Morton order, only into nodes that may hold nodes under them --
    /// a divide, or anything that masks. The residual pass says every
    /// cell of each, its four cells in Morton order -- consecutive, in the
    /// bitmap too.
    pub fn residual_squares(&mut self, tree: &Pyramid) -> &[Tile] {
        self.tiles.clear();
        self.waiting.clear();
        self.waiting.push(Tile::whole_bitmap());
        while let Some(at) = self.waiting.pop() {
            match tree.node(at) {
                Node::Residual => self.tiles.push(at),
                Node::Subdivided | Node::ComplexTile { masks: true, .. } | Node::Copied { masks: true, .. } => {
                    self.waiting.extend(at.children().into_iter().rev())
                }
                _ => {}
            }
        }
        &self.tiles
    }
}
