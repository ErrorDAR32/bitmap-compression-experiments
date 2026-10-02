//! The tree: what it holds at each tile.
//!
//! Function by function: `docs/reference.md`, "`tree.rs`".

use crate::tile::{Pyramid, Tile, FLOOR_LEVEL};

/// What the tree holds at one tile.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Node {
    /// No node: the tile lies inside a coarser node's tile, or the value
    /// bound above says it.
    #[default]
    Absent,
    /// The tile's four children, each a node of its own -- or, the
    /// divide naming its children, the value bound above saying those
    /// that are not.
    Divided,
    /// A divide naming its children that flips the value bound above:
    /// every child not a node is bound to the other value. A bind with
    /// holes.
    FlippingDivide,
    /// A copy of a same-size tile; naming its children, it copies only
    /// those that are not nodes of their own.
    Copied {
        /// Whether it reads from the far offsets.
        far: bool,
        /// Which offset it reads from.
        direction: u8,
        /// Whether some of its children are nodes of their own.
        names_children: bool,
    },
    /// A value for every tile `size_offset` levels finer, each of them
    /// homogeneous; at size offset 0, the tile bound whole.
    ComplexTile {
        /// How many levels finer than the tile its resolution is.
        size_offset: u8,
    },
    /// A complex tile of 1x1 resolution saying its set cells as a cell
    /// list.
    CellList,
    /// A 4x4 whose cells are left to the last pass.
    Residual,
}

impl Node {
    /// Whether its children's nodes follow it.
    pub fn has_children(self) -> bool {
        matches!(self, Node::Divided | Node::FlippingDivide | Node::Copied { names_children: true, .. })
    }
}

/// The tree: a node a tile, down to the 4x4 floor. Walk it from the top:
/// nodes under a complex tile are stale (`docs/tessera.md`, "Stale
/// nodes").
pub type Tree = Pyramid<Node, FLOOR_LEVEL>;

/// The value bound at the top of the bitmap, before any flipping
/// divide: clear.
pub const BOUND_AT_THE_TOP: bool = false;

/// The level the tree starts at: that of its coarsest tile that is not
/// a divide with every child a node. Every coarser tile is one -- the
/// trunk, which the stream never spells out.
pub fn start_level(tree: &Tree) -> u8 {
    (0..=FLOOR_LEVEL)
        .find(|&level| {
            Tile::all_of_level(level).any(|tile| tree.get(tile) != Node::Divided || tree.children(tile).contains(&Node::Absent))
        })
        .expect("the 4x4 floor never divides")
}
