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

pub const COPY: u64 = 0;
pub const BIND: u64 = 1;
pub const CODE_WIDTH: u8 = 1;

pub const FAR_WIDTH: u8 = 1;
pub const DIRECTION_WIDTH: u8 = 2;

/// Whether a complex tile deeper than 1, or a copy at 8x8 or coarser,
/// masks anything at all. Skipped where nothing masks: complex tiles at
/// size offsets 0 and 1, copies finer than 8x8.
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

/// How many bits name a complex tile's size offset at `level`: `0` (a tile)
/// up to a 2x2 resolution -- a 1x1 resolution never is one.
pub fn resolution_width(level: u8) -> u8 {
    let size_offsets = levels_to_cells(level);
    (u8::BITS - (size_offsets - 1).leading_zeros()) as u8
}
