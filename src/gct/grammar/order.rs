//! The order of a complex tile's payload: which parts of it the payload
//! says, walked down the tree, used by both directions, so writing and
//! reading can never disagree. (The residual pass needs no walk: its
//! 2x2s are read off the tree in Morton order,
//! `Tree::residual_squares`.)

use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{Tile, CELL_LEVEL};

/// The most tiles a walk down the tree waits on at once: three siblings
/// a level, and a last level's four.
const MOST_WAITING: usize = 3 * CELL_LEVEL as usize + 4;

/// The parts of the complex tile at `tile`, of nesting `nesting`, its
/// payload says, in payload order: the whole tile when it masks
/// nothing, otherwise every node in its body unmasked in it, in body
/// order -- including nodes inside complex tiles nested in it. The
/// payload is one value for each tile of its resolution in each part, a
/// part's in Morton order.
pub fn payload_parts(tree: &Pyramid, tile: Tile, nesting: u8) -> PayloadParts<'_> {
    let mut parts = PayloadParts { tree, nesting, whole: None, waiting: [Tile::default(); MOST_WAITING], waiting_count: 0 };
    match tree.node(tile) {
        Node::ComplexTile { masks: true, .. } => parts.wait_for_children(tile),
        _ => parts.whole = Some(tile),
    }
    parts
}

/// A payload's parts, found as they are asked for: only the tiles still
/// to visit are held, inline, never more than a walk waits on at once.
pub struct PayloadParts<'a> {
    /// The tree walked.
    tree: &'a Pyramid,
    /// The nesting of the complex tile whose payload this is.
    nesting: u8,
    /// The complex tile itself, when it masks nothing: its one part.
    whole: Option<Tile>,
    /// Tiles still to visit, the next last.
    waiting: [Tile; MOST_WAITING],
    /// How many of `waiting` are held.
    waiting_count: usize,
}

impl PayloadParts<'_> {
    /// Visits `tile`'s four children next, in reading order.
    fn wait_for_children(&mut self, tile: Tile) {
        for child in tile.children().into_iter().rev() {
            self.waiting[self.waiting_count] = child;
            self.waiting_count += 1;
        }
    }
}

impl Iterator for PayloadParts<'_> {
    type Item = Tile;

    fn next(&mut self) -> Option<Tile> {
        if let Some(whole) = self.whole.take() {
            return Some(whole);
        }
        while self.waiting_count > 0 {
            self.waiting_count -= 1;
            let body_tile = self.waiting[self.waiting_count];
            match self.tree.node(body_tile) {
                Node::Unmasked { nesting } if nesting == self.nesting => return Some(body_tile),
                Node::Subdivided | Node::ComplexTile { masks: true, .. } | Node::Copied { masks: true, .. } => {
                    self.wait_for_children(body_tile)
                }
                _ => {}
            }
        }
        None
    }
}
