//! The tree the complex tiler produces, held as a pyramid: one 8-bit
//! node code per tile, levels 0 (the whole bitmap) to 4x4. Every node
//! sits at exactly one tile, so no pointers are needed -- a tile's node
//! is one lookup away, and its children's nodes are its children's
//! elements. Tiles inside a coarser leaf, or finer than the 4x4 floor,
//! hold no node.
//!
//! The whole plane is tiled with complex tiles: every placed `Bound`
//! tile is either unmasked in a complex tile it is nested in or is a
//! complex tile whose resolution is its own size -- just a *tile*
//! (`ComplexTile` at size offset 0). A bind is always a complex tile.
//!
//! Values are not held here: an unmasked tile's values are the cells of
//! its resolution tiles, which are the bitmap's own.

use crate::gct::pyramids::pyramid::{Pyramid, PyramidShape};
use crate::gct::grammar::{DIRECTION_MASK, DIRECTION_WIDTH, FAR_WIDTH};
use crate::gct::tile::{Tile, LEVEL_BITS, FLOOR_LEVEL};
use crate::morton::morton_coordinates;

/// What the tree holds at one tile.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Node {
    /// No node at this tile: it lies inside a coarser node's tile -- a
    /// leaf's, or a child a masking copy says -- or the binding above it
    /// says it, or it is finer than the 4x4 floor.
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
    /// size offset 0 masking, a bind that masks: it binds under it, and
    /// its children holding a node are masked in it. When it `masks`
    /// nothing, every tile of its resolution is unmasked in it (always so
    /// at size offsets 0 and 1); when it does, its four children hold its
    /// body.
    ComplexTile {
        /// How many levels finer than the tile its resolution is.
        size_offset: u8,
        /// Whether some of what it holds is masked in it.
        masks: bool,
    },
    /// A residual block: a 4x4 that is not one bound tile, a copy or a
    /// complex tile, its cells left to the last pass.
    Residual,
    /// A complex tile of 1x1 resolution masking nothing, saying its set
    /// cells as a [cell list](crate::gct::grammar::cell_list).
    CellList,
}

/// Bits a node's kind takes, at the bottom of its code: enough for the
/// seven kinds.
const KIND_BITS: u64 = 3;
/// A kind's bits.
const KIND_MASK: u64 = (1 << KIND_BITS) - 1;
/// Where the kind's parameter starts: right after it.
const PARAMETER_SHIFT: u64 = KIND_BITS;
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
/// The kind of [`Node::CellList`].
const CELL_LIST: u64 = 6;
/// Copied: far in the parameter's bottom bit...
const FAR: u64 = 1;
/// ...its direction right above...
const DIRECTION_SHIFT: u64 = FAR_WIDTH as u64;
/// ...and whether it masks above that.
const COPY_MASKS: u64 = 1 << (DIRECTION_SHIFT + DIRECTION_WIDTH as u64);
/// Complex tile: the size offset at the bottom of the parameter, up to
/// a level's worth (the whole bitmap at 1x1)...
const SIZE_OFFSET_MASK: u64 = (1 << LEVEL_BITS) - 1;
/// ...and whether it masks right above.
const COMPLEX_TILE_MASKS: u64 = 1 << LEVEL_BITS;
/// Unmasked: the nesting, the parameter's bottom bits -- fewer nestings
/// than levels.
const NESTING_MASK: u64 = (1 << LEVEL_BITS) - 1;

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
        Node::CellList => (CELL_LIST, 0),
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
        CELL_LIST => Node::CellList,
        kind => unreachable!("no node kind {kind}"),
    }
}

/// Bits a node's code takes.
const NODE_BITS: usize = 8;
/// One node code, at the bottom of a word.
const NODE_MASK: u64 = (1 << NODE_BITS) - 1;
/// Node codes a word holds.
const NODES_A_WORD: usize = u64::BITS as usize / NODE_BITS;
/// A residual block's code: its kind, with no parameter.
const RESIDUAL_CODE: u64 = RESIDUAL;

/// One node code a tile, 8 bits, down to the 4x4 floor: nothing finer
/// is ever a node.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TreeShape;

impl PyramidShape for TreeShape {
    const COARSEST_LEVEL: u8 = 0;
    const FINEST_LEVEL: u8 = FLOOR_LEVEL;
    const ELEMENT_BITS: usize = NODE_BITS;
}

/// The tree: a node code a tile.
pub type Tree = Pyramid<TreeShape, { TreeShape::WORDS }>;

impl Tree {
    /// The node at `tile`.
    pub fn node(&self, tile: Tile) -> Node {
        from_code(self.get(tile))
    }

    /// Makes `node` the node at `tile`.
    pub fn set_node(&mut self, tile: Tile, node: Node) {
        self.set(tile, to_code(node));
    }

    /// Whether `tile` is `Subdivided` with every child a node of its own:
    /// a divide leaving nothing to the binding above.
    pub fn divides_whole(&self, tile: Tile) -> bool {
        self.node(tile) == Node::Subdivided && tile.children().into_iter().all(|child| self.node(child) != Node::Absent)
    }

    /// The residual blocks, in Morton order: the 4x4 level's node codes
    /// read a word at a time, a word of no node skipped whole.
    pub fn residual_blocks(&self) -> impl Iterator<Item = Tile> + '_ {
        let level = FLOOR_LEVEL;
        self.level_words(level).iter().enumerate().filter(|&(_, &word)| word != 0).flat_map(move |(word_index, &word)| {
            (0..NODES_A_WORD).filter(move |&slot| (word >> (slot * NODE_BITS)) & NODE_MASK == RESIDUAL_CODE).map(move |slot| {
                let (x, y) = morton_coordinates(word_index * NODES_A_WORD + slot);
                Tile { level, x, y }
            })
        })
    }

    /// The level the tree starts at: that of its coarsest node that does
    /// not divide whole. Every tile coarser than it does -- the trunk,
    /// which the stream never spells out.
    pub fn start_level(&self) -> u8 {
        (0..=FLOOR_LEVEL)
            .find(|&level| Tile::all_of_level(level).any(|tile| !self.divides_whole(tile)))
            .expect("the 4x4 floor never subdivides")
    }
}
