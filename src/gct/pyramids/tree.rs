//! The tree the complex tiler produces, held as a pyramid: one 8-bit
//! node code per tile, levels 0 (the whole bitmap) to 2x2. Every node
//! sits at exactly one tile, so no pointers are needed -- a tile's node
//! is one lookup away, and its children's nodes are its children's
//! elements. Tiles inside a coarser leaf, or finer than the 2x2 floor,
//! hold no node.
//!
//! The whole plane is tiled with complex tiles: every placed `Bound`
//! tile is either related to a complex tile enclosing it or is a
//! complex tile whose resolution is its own size -- just a *tile*
//! (`Complex` at size offset 0). A bind is always a complex tile.
//!
//! Values are not held here: a related tile's values are the cells of
//! its resolution tiles, which are the bitmap's own.

use crate::gct::pyramids::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{Tile, CELL_LEVEL};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Node {
    /// No node here: inside a coarser leaf, or never reached.
    None,
    /// The same question asked again of this tile's four children.
    Split,
    /// Related to (unmasked in) one of the complex tiles enclosing this
    /// tile, `nesting` naming which (`0` the outermost): one value per
    /// tile of its resolution under this one, said in its payload.
    Related { nesting: usize },
    /// One placed tile, copying a same-size area.
    Copied { far: bool, direction: usize },
    /// A complex tile whose resolution is `size_offset` levels finer. Without
    /// `masking`, every tile of its resolution is related to it (always
    /// so at size offsets 0 and 1); with it, its four children hold its body.
    Complex { size_offset: usize, masking: bool },
    /// A 2x2 that is not one placed `Bound` tile: its four cells are
    /// left to the residual pass.
    Hole,
}

/// Bits 0-2: which kind. Bits 3-6: that kind's parameter.
const KIND_MASK: u64 = 0b111;
const PARAMETER_SHIFT: u64 = 3;
const SPLIT: u64 = 1;
const RELATED: u64 = 2;
const COPIED: u64 = 3;
const COMPLEX: u64 = 4;
const HOLE: u64 = 5;
/// Copied: far in parameter bit 0, direction in bits 1-2.
const FAR: u64 = 0b1;
const DIRECTION_SHIFT: u64 = 1;
const DIRECTION_MASK: u64 = 0b11;
/// Complex: size offset in parameter bits 0-2, masking in bit 3.
const SIZE_OFFSET_MASK: u64 = 0b111;
const MASKING: u64 = 0b1000;
const NESTING_MASK: u64 = 0b1111;

fn to_code(node: Node) -> u64 {
    let (kind, parameter) = match node {
        Node::None => (0, 0),
        Node::Split => (SPLIT, 0),
        Node::Related { nesting } => {
            assert!(nesting as u64 <= NESTING_MASK, "nesting {nesting} does not fit a node code");
            (RELATED, nesting as u64)
        }
        Node::Copied { far, direction } => (COPIED, far as u64 | (direction as u64) << DIRECTION_SHIFT),
        Node::Complex { size_offset, masking } => (COMPLEX, size_offset as u64 | if masking { MASKING } else { 0 }),
        Node::Hole => (HOLE, 0),
    };
    kind | parameter << PARAMETER_SHIFT
}

fn from_code(code: u64) -> Node {
    let parameter = code >> PARAMETER_SHIFT;
    match code & KIND_MASK {
        0 => Node::None,
        SPLIT => Node::Split,
        RELATED => Node::Related { nesting: (parameter & NESTING_MASK) as usize },
        COPIED => Node::Copied {
            far: parameter & FAR != 0,
            direction: ((parameter >> DIRECTION_SHIFT) & DIRECTION_MASK) as usize,
        },
        COMPLEX => Node::Complex { size_offset: (parameter & SIZE_OFFSET_MASK) as usize, masking: parameter & MASKING != 0 },
        HOLE => Node::Hole,
        kind => unreachable!("no node kind {kind}"),
    }
}

const SHAPE: PyramidShape = PyramidShape { arity: 4, coarsest_level: 0, finest_level: CELL_LEVEL - 1, element_bits: 8 };

pub trait Tree {
    /// A tree with no nodes yet.
    fn tree() -> Self;

    fn node(&self, tile: Tile) -> Node;

    fn set_node(&mut self, tile: Tile, node: Node);
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
}
