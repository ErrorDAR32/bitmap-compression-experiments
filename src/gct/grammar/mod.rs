//! The grammar, shared by both directions: what every bit means, and
//! the order a payload's value bits go in. [`crate::gct::encode`](mod@crate::gct::encode)
//! writes it and [`crate::gct::decode`](mod@crate::gct::decode) reads it; neither holds a rule
//! of its own. The full grammar, with its costs, is in `docs/gct.md`.
//!
//! A stream is its mode, then either the tree -- the start level, the
//! tree node by node from every tile of that level in Morton order (each
//! complex tile's payload right after its body), then the residual pass
//! -- or, when it takes fewer bits, the bitmap's
//! [count split](count_split): sparse cells, with no whole areas and
//! nothing to copy, which the tree says poorly.

pub mod bit_stream;
pub mod cell_list;
pub mod count_split;
pub mod order;

pub use crate::gct::pyramids::placements::BOUND_AT_THE_TOP;
use crate::gct::pyramids::placements::FINEST_MASKING_LEVEL;
use crate::gct::tile::{cells_in_tile, levels_to_cells, CELL_LEVEL, CHILDREN, DIRECTIONS, FLOOR_LEVEL};

/// The stream's mode when the tree follows...
pub const TREE_STREAM: u64 = 0;
/// ...and when the bitmap's count split follows instead: only when it
/// takes strictly fewer bits than the tree.
pub const COUNT_SPLIT_STREAM: u64 = 1;
/// Bits in the stream's mode.
pub const STREAM_MODE_WIDTH: u8 = 1;

/// A residual 2x2's bits in the residual pass: one a cell, in Morton
/// order -- as its cells lie in the bitmap, so read or written as one
/// value.
pub const RESIDUAL_SQUARE_BITS: u8 = cells_in_tile(FLOOR_LEVEL) as u8;

/// The level the tree starts at, whole bitmap (0) to the 2x2 floor:
/// every coarser tile subdivides, so none of them is written.
pub const START_LEVEL_WIDTH: u8 = (u8::BITS - (FLOOR_LEVEL).leading_zeros()) as u8;

/// One mask bit per complex tile a node is nested in that could unmask
/// it, nearest first: this one means the node is unmasked in that
/// complex tile -- its value is in the complex tile's payload, and the
/// node ends here.
pub const UNMASKED: u64 = 0;
/// A mask bit meaning the node is masked in that complex tile: the next
/// complex tile out is asked, or, after the last, the node itself
/// follows.
pub const MASKED: u64 = 1;
/// Bits in one mask bit.
pub const MASK_BIT_WIDTH: u8 = 1;

/// The leaf bit's value for a leaf: a copy or a bind follows, or, at
/// the 2x2 floor, a tile and its value.
pub const LEAF: u64 = 1;
/// The leaf bit's value for a divide, above the 2x2 floor: the node's
/// children follow, or, at 8x8 and coarser, first its mask-present bit.
pub const SUBDIVIDE: u64 = 0;
/// The leaf bit's value at the 2x2 floor for a residual: its four cells
/// are left to the residual pass.
pub const RESIDUAL: u64 = 0;
/// Bits in the leaf bit.
pub const LEAF_WIDTH: u8 = 1;

/// A divide that masks keeps the value bound above it for the children
/// it leaves unnamed.
pub const BINDING_KEPT: u64 = 0;
/// A divide that masks flips the value bound above it for the children
/// it leaves unnamed: a bind that masks.
pub const BINDING_FLIPPED: u64 = 1;
/// Bits in the flip bit.
pub const FLIP_WIDTH: u8 = 1;
/// Bits in a child mask: a mask bit a child.
pub const CHILD_MASK_WIDTH: u8 = CHILDREN * MASK_BIT_WIDTH;
/// What a divide that masks, or a bind that masks, spells before its
/// child mask: its leaf bit, its mask-present bit and its flip bit.
pub const MASKING_DIVIDE_HEADER_WIDTH: u8 = LEAF_WIDTH + MASK_PRESENT_WIDTH + FLIP_WIDTH;

/// The code bit after a leaf bit for a copy: far, direction and, at
/// 8x8 and coarser, a mask-present bit follow.
pub const COPY: u64 = 0;
/// The code bit after a leaf bit for a bind: a complex tile's size
/// offset, its mask-present bit where it may mask, its body and its
/// payload follow.
pub const BIND: u64 = 1;
/// Bits in the code bit.
pub const CODE_WIDTH: u8 = 1;

/// Bits saying whether a copy is far (a neighbour of the tile's parent)
/// or near (a neighbour of the tile itself).
pub const FAR_WIDTH: u8 = 1;
/// Bits naming a copy's direction: one of the four in
/// [`DIRECTIONS`].
pub const DIRECTION_WIDTH: u8 = DIRECTIONS.len().trailing_zeros() as u8;
const _: () = assert!(DIRECTIONS.len().is_power_of_two());
/// A direction's bits, at the bottom of a word.
pub const DIRECTION_MASK: u64 = (1 << DIRECTION_WIDTH) - 1;

/// The mask-present bit's value when a node masks nothing at all. The
/// bit is skipped where nothing may mask: complex tiles at size offsets
/// 0 and 1 or at a 1x1 resolution, copies and divides finer than 8x8.
pub const NO_MASKING: u64 = 0;
/// The mask-present bit's value when a node masks some of what it
/// holds: its child mask, or its children's mask bits, follow.
pub const MASKING: u64 = 1;
/// Bits in the mask-present bit.
pub const MASK_PRESENT_WIDTH: u8 = 1;

/// The payload mode of a complex tile of 1x1 resolution masking nothing:
/// every cell raw, one bit each...
pub const PLAIN_PAYLOAD: u64 = 0;
/// ...or a [cell list](cell_list) of its set cells.
pub const CELL_LIST: u64 = 1;
/// Bits in the payload mode.
pub const PAYLOAD_MODE_WIDTH: u8 = 1;

/// Whether a complex tile at `level` of `size_offset`, masking or not
/// as `masks` says, has a payload mode: when it is of 1x1 resolution and
/// masks nothing. It follows the mask-present bit.
pub fn has_payload_mode(level: u8, size_offset: u8, masks: bool) -> bool {
    level + size_offset == CELL_LEVEL && !masks
}

/// Whether a copy or a divide at `level` has a mask-present bit: down to
/// 8x8, as masking 2x2s saves less than its mask costs. A masking copy's
/// child mask follows it: one mask bit per child, in reading order --
/// [`UNMASKED`] said by the copy, [`MASKED`] a node of its own, which
/// follow in that order.
pub fn copy_or_divide_may_mask(level: u8) -> bool {
    level <= FINEST_MASKING_LEVEL
}

/// Whether a complex tile of `size_offset` has a mask-present bit: not
/// at size offsets 0 and 1.
pub fn complex_tile_may_mask(size_offset: u8) -> bool {
    size_offset > 1
}

/// How many bits name a complex tile's size offset at `level`: enough
/// for `0` (a tile) up to a 2x2 resolution.
pub fn resolution_width(level: u8) -> u8 {
    let largest = levels_to_cells(level) - 1;
    (u8::BITS - largest.leading_zeros()) as u8
}

/// Whether a 1x1 resolution -- a complex tile saying every cell under it
/// raw, the escape for what nothing else compresses -- can be named at
/// `level`: where the size offset field has a value to spare for it
/// (128x128, 64x64, 32x32 and 8x8). It is never worth widening the field
/// every other tile of that size pays.
pub fn raw_resolution_fits(level: u8) -> bool {
    u32::from(levels_to_cells(level)) < 1 << resolution_width(level)
}
