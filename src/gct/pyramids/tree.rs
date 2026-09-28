//! The tree the complex tiler produces, held as a pyramid: one 8-bit
//! node code per tile, levels 0 (the whole bitmap) to 2x2. Every node
//! sits at exactly one tile, so no pointers are needed -- a tile's node
//! is one lookup away, and its children's nodes are its children's
//! elements. Tiles inside a coarser leaf, or finer than the 2x2 floor,
//! hold no node.
//!
//! The whole plane is tiled with complex tiles: every placed `Bound`
//! tile is either unmasked in a complex tile it is nested in or is a
//! complex tile whose resolution is its own size -- just a *tile*
//! (`Complex` at size offset 0). A bind is always a complex tile.
//!
//! Values are not held here: an unmasked tile's values are the cells of
//! its resolution tiles, which are the bitmap's own.

use crate::gct::pyramids::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{Tile, CELL_LEVEL};

/// What the tree holds at one tile.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Node {
    /// No node at this tile: it lies inside a coarser node's tile -- a
    /// leaf's, or a child a masking copy says -- or the binding above
    /// above it says it, or it is finer than the 2x2 floor.
    Absent,
    /// The same question asked again of this tile's four children -- of
    /// those holding a node; the binding above says the others.
    Subdivided,
    /// Unmasked in one of the complex tiles this tile is nested in,
    /// `nesting` naming which (`0` the outermost): one value per tile of
    /// its resolution under this one, bound in that complex tile's
    /// payload.
    Unmasked {
        /// Which enclosing complex tile unmasks it, `0` the outermost.
        nesting: u8,
    },
    /// One placed tile, copying a same-size area. When it `masks`, it
    /// says only its children holding no node: each child with a node
    /// is masked in it, and said by that node.
    Copied {
        /// Whether it copies a neighbour of its parent rather than of
        /// itself.
        far: bool,
        /// Which of [`DIRECTIONS`](crate::gct::tile::DIRECTIONS) it
        /// copies from.
        direction: u8,
        /// Whether some of its children are masked in it.
        masks: bool,
    },
    /// A complex tile whose resolution is `size_offset` levels finer. At
    /// size offset 0 masking, a bind that masks: it binds under
    /// it, and its children holding a node are masked in it.
    /// When it `masks` nothing, every tile of its resolution is unmasked
    /// in it (always so at size offsets 0 and 1); when it does, its four
    /// children hold its body.
    ComplexTile {
        /// How many levels finer than the tile its resolution is.
        size_offset: u8,
        /// Whether some of what it holds is masked in it.
        masks: bool,
    },
    /// A 2x2 that is not one bound tile: its four cells are left to the
    /// residual pass.
    Residual,
}

/// Bits 0-2 of a node's code: which kind.
const KIND_MASK: u64 = 0b111;
/// Where that kind's parameter starts: bits 3 and up.
const PARAMETER_SHIFT: u64 = 3;
/// The kind of [`Node::Absent`]; also an all-zero element, so a fresh
/// tree holds no nodes.
const ABSENT: u64 = 0;
/// The kind of [`Node::Subdivided`].
const SUBDIVIDED: u64 = 1;
/// The kind of [`Node::Unmasked`].
const UNMASKED: u64 = 2;
/// The kind of [`Node::Copied`].
const COPIED: u64 = 3;
/// The kind of [`Node::ComplexTile`].
const COMPLEX_TILE: u64 = 4;
/// The kind of [`Node::Residual`].
const RESIDUAL: u64 = 5;
/// Copied: far in parameter bit 0.
const FAR: u64 = 0b1;
/// Copied: direction in parameter bits 1-2.
const DIRECTION_SHIFT: u64 = 1;
/// Copied: the direction's two bits, once shifted down.
const DIRECTION_MASK: u64 = 0b11;
/// Copied: masks in parameter bit 3.
const COPY_MASKS: u64 = 0b1000;
/// Complex tile: size offset in parameter bits 0-3 (up to 8, the whole
/// bitmap at 1x1), masks in bit 4.
const SIZE_OFFSET_MASK: u64 = 0b1111;
/// Complex tile: masks in parameter bit 4.
const COMPLEX_TILE_MASKS: u64 = 0b1_0000;
/// Unmasked: the nesting in parameter bits 0-3.
const NESTING_MASK: u64 = 0b1111;

/// A node's code, as the tree pyramid holds it.
fn to_code(node: Node) -> u64 {
    let (kind, parameter) = match node {
        Node::Absent => (ABSENT, 0),
        Node::Subdivided => (SUBDIVIDED, 0),
        Node::Unmasked { nesting } => {
            assert!(nesting as u64 <= NESTING_MASK, "nesting {nesting} does not fit a node code");
            (UNMASKED, nesting as u64)
        }
        Node::Copied { far, direction, masks } => {
            (COPIED, far as u64 | (direction as u64) << DIRECTION_SHIFT | if masks { COPY_MASKS } else { 0 })
        }
        Node::ComplexTile { size_offset, masks } => {
            (COMPLEX_TILE, size_offset as u64 | if masks { COMPLEX_TILE_MASKS } else { 0 })
        }
        Node::Residual => (RESIDUAL, 0),
    };
    kind | parameter << PARAMETER_SHIFT
}

/// The node a code names.
fn from_code(code: u64) -> Node {
    let parameter = code >> PARAMETER_SHIFT;
    match code & KIND_MASK {
        ABSENT => Node::Absent,
        SUBDIVIDED => Node::Subdivided,
        UNMASKED => Node::Unmasked { nesting: (parameter & NESTING_MASK) as u8 },
        COPIED => Node::Copied {
            far: parameter & FAR != 0,
            direction: ((parameter >> DIRECTION_SHIFT) & DIRECTION_MASK) as u8,
            masks: parameter & COPY_MASKS != 0,
        },
        COMPLEX_TILE => Node::ComplexTile {
            size_offset: (parameter & SIZE_OFFSET_MASK) as u8,
            masks: parameter & COMPLEX_TILE_MASKS != 0,
        },
        RESIDUAL => Node::Residual,
        kind => unreachable!("no node kind {kind}"),
    }
}

/// One node code a tile, 8 bits, down to the 2x2 floor: nothing finer
/// is ever a node.
const SHAPE: PyramidShape = PyramidShape { coarsest_level: 0, finest_level: CELL_LEVEL - 1, element_bits: 8 };

/// The tree's queries and updates, over the node codes.
pub trait Tree {
    /// A tree with no nodes yet.
    fn tree() -> Self;

    /// The node at `tile`.
    fn node(&self, tile: Tile) -> Node;

    /// Makes `node` the node at `tile`.
    fn set_node(&mut self, tile: Tile, node: Node);

    /// Whether `tile` is `Subdivided` with every child a node of its own:
    /// a divide leaving nothing to the binding above.
    fn divides_whole(&self, tile: Tile) -> bool;

    /// The level the tree starts at: that of its coarsest node that does
    /// not divide whole. Every tile coarser than it does -- the trunk,
    /// which the stream never spells out.
    fn start_level(&self) -> u8;
}

impl Tree for Pyramid {
    fn tree() -> Self {
        Pyramid::new(SHAPE)
    }

    fn node(&self, tile: Tile) -> Node {
        from_code(self.get(tile))
    }

    fn set_node(&mut self, tile: Tile, node: Node) {
        self.set(tile, to_code(node));
    }

    fn divides_whole(&self, tile: Tile) -> bool {
        self.node(tile) == Node::Subdivided && tile.children().into_iter().all(|child| self.node(child) != Node::Absent)
    }

    fn start_level(&self) -> u8 {
        (0..=SHAPE.finest_level)
            .find(|&level| Tile::all_of_level(level).any(|tile| !self.divides_whole(tile)))
            .expect("the 2x2 floor never subdivides")
    }
}
