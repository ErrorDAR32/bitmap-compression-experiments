//! The grammar, shared by both directions: what every bit means, and
//! the order the plain runs of value bits go in. [`crate::gct::encode`](mod@crate::gct::encode)
//! writes it and [`crate::gct::decode`](mod@crate::gct::decode) reads it; neither holds a rule
//! of its own. The full grammar, with its costs, is in `docs/gct.md`.
//!
//! A stream is the start level, then the tree, node by node from every
//! tile of that level in reading order (each complex tile's payload
//! right after its body), then the residual pass.

pub mod bit_stream;
pub mod order;

pub use crate::gct::pyramids::placements::BOUND_AT_THE_TOP;
use crate::gct::pyramids::placements::FINEST_MASKING_LEVEL;
use crate::gct::tile::{levels_to_cells, CELL_LEVEL};

/// The level the tree starts at, whole bitmap (0) to the 2x2 floor:
/// every coarser tile subdivides, so none of them is written.
pub const START_LEVEL_WIDTH: u8 = (u8::BITS - (CELL_LEVEL - 1).leading_zeros()) as u8;

/// One mask bit per complex tile a node is nested in that could unmask
/// it, nearest first: unmasked in it, or masked.
pub const UNMASKED: u64 = 0;
pub const MASKED: u64 = 1;
pub const MASK_BIT_WIDTH: u8 = 1;

pub const LEAF: u64 = 1;
pub const SUBDIVIDE: u64 = 0;
pub const RESIDUAL: u64 = 0;
pub const LEAF_WIDTH: u8 = 1;

/// Whether a divide that masks keeps the value bound above it for
/// the children it leaves unnamed, or flips it -- a bind that masks.
pub const BINDING_KEPT: u64 = 0;
pub const BINDING_FLIPPED: u64 = 1;
pub const FLIP_WIDTH: u8 = 1;

pub const COPY: u64 = 0;
pub const BIND: u64 = 1;
pub const CODE_WIDTH: u8 = 1;

pub const FAR_WIDTH: u8 = 1;
pub const DIRECTION_WIDTH: u8 = 2;

/// Whether a complex tile deeper than 1, or a copy at 8x8 or coarser,
/// masks anything at all. Skipped where nothing masks: complex tiles at
/// size offsets 0 and 1 or at a 1x1 resolution, copies finer than 8x8.
pub const NO_MASKING: u64 = 0;
pub const MASKING: u64 = 1;
pub const MASK_PRESENT_WIDTH: u8 = 1;

/// Whether a copy at `level` has a mask-present bit. A masking copy's
/// child mask follows it: one mask bit per child, in reading order --
/// [`UNMASKED`] said by the copy, [`MASKED`] a node of its own, which
/// follow in that order.
pub fn copy_may_mask(level: u8) -> bool {
    level <= FINEST_MASKING_LEVEL
}

/// Whether a divide at `level` has a mask-present bit -- down to 8x8,
/// as for copies: a divide masking 2x2s saves less than its mask costs.
pub fn divide_may_mask(level: u8) -> bool {
    level <= FINEST_MASKING_LEVEL
}

/// Whether a complex tile at `level` of `size_offset` has a
/// mask-present bit: not at size offsets 0 and 1.
pub fn complex_tile_may_mask(_level: u8, size_offset: u8) -> bool {
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
