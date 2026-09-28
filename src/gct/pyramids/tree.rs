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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Node {
    /// No node at this tile: it lies inside a coarser node's tile, or is
    /// finer than the 2x2 floor.
    Absent,
    /// The same question asked again of this tile's four children.
    Subdivided,
    /// Unmasked in one of the complex tiles this tile is nested in,
    /// `nesting` naming which (`0` the outermost): one value per tile of
    /// its resolution under this one, bound in that complex tile's
    /// payload.
    Unmasked { nesting: u8 },
    /// One placed tile, copying a same-size area.
    Copied { far: bool, direction: u8 },
    /// A complex tile whose resolution is `size_offset` levels finer.
    /// When it `masks` nothing, every tile of its resolution is unmasked
    /// in it (always so at size offsets 0 and 1); when it does, its four
    /// children hold its body.
    ComplexTile { size_offset: u8, masks: bool },
    /// A 2x2 that is not one bound tile: its four cells are left to the
    /// residual pass.
    Residual,
}

/// Bits 0-2: which kind. Bits 3-6: that kind's parameter.
const KIND_MASK: u64 = 0b111;
const ABSENT: u64 = 0;
const PARAMETER_SHIFT: u64 = 3;
const SUBDIVIDED: u64 = 1;
const UNMASKED: u64 = 2;
const COPIED: u64 = 3;
const COMPLEX_TILE: u64 = 4;
const RESIDUAL: u64 = 5;
/// Copied: far in parameter bit 0, direction in bits 1-2.
const FAR: u64 = 0b1;
const DIRECTION_SHIFT: u64 = 1;
const DIRECTION_MASK: u64 = 0b11;
/// Complex tile: size offset in parameter bits 0-2, masks in bit 3.
const SIZE_OFFSET_MASK: u64 = 0b111;
const MASKS: u64 = 0b1000;
const NESTING_MASK: u64 = 0b1111;

fn to_code(node: Node) -> u64 {
    let (kind, parameter) = match node {
        Node::Absent => (ABSENT, 0),
        Node::Subdivided => (SUBDIVIDED, 0),
        Node::Unmasked { nesting } => {
            assert!(nesting as u64 <= NESTING_MASK, "nesting {nesting} does not fit a node code");
            (UNMASKED, nesting as u64)
        }
        Node::Copied { far, direction } => (COPIED, far as u64 | (direction as u64) << DIRECTION_SHIFT),
        Node::ComplexTile { size_offset, masks } => (COMPLEX_TILE, size_offset as u64 | if masks { MASKS } else { 0 }),
        Node::Residual => (RESIDUAL, 0),
    };
    kind | parameter << PARAMETER_SHIFT
}

fn from_code(code: u64) -> Node {
    let parameter = code >> PARAMETER_SHIFT;
    match code & KIND_MASK {
        ABSENT => Node::Absent,
        SUBDIVIDED => Node::Subdivided,
        UNMASKED => Node::Unmasked { nesting: (parameter & NESTING_MASK) as u8 },
        COPIED => Node::Copied {
            far: parameter & FAR != 0,
            direction: ((parameter >> DIRECTION_SHIFT) & DIRECTION_MASK) as u8,
        },
        COMPLEX_TILE => Node::ComplexTile { size_offset: (parameter & SIZE_OFFSET_MASK) as u8, masks: parameter & MASKS != 0 },
        RESIDUAL => Node::Residual,
        kind => unreachable!("no node kind {kind}"),
    }
}

const SHAPE: PyramidShape = PyramidShape { arity: 4, coarsest_level: 0, finest_level: CELL_LEVEL - 1, element_bits: 8 };

pub trait Tree {
    /// A tree with no nodes yet.
    fn tree() -> Self;

    fn node(&self, tile: Tile) -> Node;

    fn set_node(&mut self, tile: Tile, node: Node);

    /// The level the tree starts at: that of its coarsest node that is
    /// not `Subdivided`. Every tile coarser than it subdivides -- the
    /// trunk, which the stream never spells out.
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

    fn start_level(&self) -> u8 {
        (0..=SHAPE.finest_level)
            .find(|&level| Tile::all_of_level(level).any(|tile| self.node(tile) != Node::Subdivided))
            .expect("the 2x2 floor never subdivides")
    }
}
