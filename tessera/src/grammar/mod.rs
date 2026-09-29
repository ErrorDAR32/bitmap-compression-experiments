//! The grammar, shared by both directions: what every bit means, and
//! the order a payload's value bits go in. [`crate::encode`](mod@crate::encode)
//! writes it and [`crate::decode`](mod@crate::decode) reads it; neither holds a rule
//! of its own. The full grammar, with its costs, is in `docs/tessera.md`.
//!
//! A stream is its mode, then either the tree -- the start level, the
//! tree node by node from every tile of that level in Morton order (each
//! complex tile's payload right after it), then the
//! [last pass](crate::last_pass) -- or, when it takes fewer bits,
//! the bitmap's
//! [count split](count_split): sparse cells, with no whole areas and
//! nothing to copy, which the tree says poorly.

pub mod arithmetic;
pub mod bit_stream;
pub mod cell_list;
pub mod count_split;

use bit_stream::{truncated_binary_bits, BitReader, BitStream};
pub use crate::pyramids::placements::BOUND_AT_THE_TOP;
use crate::pyramids::placements::FINEST_MASKING_LEVEL;
use crate::tile::{cells_in_tile, levels_to_cells, CELL_LEVEL, CHILDREN, DIRECTIONS, FLOOR_LEVEL};

/// The stream's mode when the tree follows...
pub const TREE_STREAM: u64 = 0;
/// ...and when the bitmap's count split follows instead: only when it
/// takes strictly fewer bits than the tree.
pub const COUNT_SPLIT_STREAM: u64 = 1;
/// Bits in the stream's mode.
pub const STREAM_MODE_WIDTH: u8 = 1;

/// A residual block's bits in the last pass: one a cell, in Morton order
/// -- as its cells lie in the bitmap, so read or written as one value.
pub const RESIDUAL_BLOCK_BITS: u8 = cells_in_tile(FLOOR_LEVEL) as u8;

/// The level the tree starts at, whole bitmap (0) to the 4x4 floor:
/// every coarser tile subdivides, so none of them is written.
pub const START_LEVEL_WIDTH: u8 = (u8::BITS - (FLOOR_LEVEL).leading_zeros()) as u8;

/// A child mask's bit for a child the masking node says itself: left to
/// the binding above, or copied with it.
pub const UNMASKED: u64 = 0;
/// A child mask's bit for a child that is a node of its own, which
/// follows.
pub const MASKED: u64 = 1;
/// Bits in one mask bit.
pub const MASK_BIT_WIDTH: u8 = 1;

/// The leaf bit's value for a leaf: a copy or a bind follows.
pub const LEAF: u64 = 1;
/// The leaf bit's value for a divide, above the 4x4 floor: the node's
/// children follow, or, at 8x8 and coarser, first its mask-present bit.
pub const SUBDIVIDE: u64 = 0;
/// The leaf bit's value at the 4x4 floor for a residual block: its
/// cells are left to the last pass.
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
/// The code bit after a leaf bit for a bind: its size offset
/// ([`push_size_offset`]) and its payload follow.
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

/// The mask-present bit's value when a copy or a divide masks none of
/// its children. The bit is skipped where nothing may mask: copies and
/// divides finer than 8x8. Complex tiles never mask.
pub const NO_MASKING: u64 = 0;
/// The mask-present bit's value when a copy or a divide masks some of
/// its children: its child mask follows.
pub const MASKING: u64 = 1;
/// Bits in the mask-present bit.
pub const MASK_PRESENT_WIDTH: u8 = 1;

/// The payload mode of a complex tile of 1x1 resolution: every cell
/// raw, one bit each...
pub const PLAIN_PAYLOAD: u64 = 0;
/// ...or a [cell list](cell_list) of its set cells.
pub const CELL_LIST: u64 = 1;
/// Bits in the payload mode.
pub const PAYLOAD_MODE_WIDTH: u8 = 1;

/// Whether a complex tile at `level` of `size_offset` has a payload
/// mode: when it is of 1x1 resolution. It follows the size offset.
pub fn has_payload_mode(level: u8, size_offset: u8) -> bool {
    level + size_offset == CELL_LEVEL
}

/// Whether a copy or a divide at `level` has a mask-present bit: down to
/// 8x8, as masking 2x2s saves less than its mask costs. A masking copy's
/// child mask follows it: one mask bit per child, in reading order --
/// [`UNMASKED`] said by the copy, [`MASKED`] a node of its own, which
/// follow in that order.
pub fn copy_or_divide_may_mask(level: u8) -> bool {
    level <= FINEST_MASKING_LEVEL
}

/// A bind's size offset starts with this for a tile: size offset 0,
/// and nothing more...
pub const TILE: u64 = 0;
/// ...or with this for a complex tile: its size offset follows, in
/// truncated binary over the size offsets its level allows, finest
/// first -- the finest are what complex tiles are mostly made at, 1x1
/// above all, and they take the short codes. A tile, far the most
/// common bind, takes one bit.
pub const COMPLEX: u64 = 1;
/// Bits in the tile-or-complex bit.
pub const TILE_OR_COMPLEX_WIDTH: u8 = 1;

/// The largest size offset a complex tile at `level` can have: down to
/// 2x2, or to 1x1 -- saying every cell under it raw, the escape for what
/// nothing else compresses -- where a fixed-width field would have had
/// a value to spare for it (128x128, 64x64, 32x32 and 8x8).
pub const fn most_size_offset(level: u8) -> u8 {
    let to_cells = levels_to_cells(level);
    let to_finest_placed = to_cells - 1;
    let width = u8::BITS - to_finest_placed.leading_zeros();
    if (to_cells as u32) < 1 << width { to_cells } else { to_finest_placed }
}

/// The bits a bind's size offset `size_offset` at `level` takes.
pub const fn size_offset_bits(level: u8, size_offset: u8) -> u8 {
    if size_offset == 0 {
        return TILE_OR_COMPLEX_WIDTH;
    }
    TILE_OR_COMPLEX_WIDTH + truncated_binary_bits(finest_first(level, size_offset), most_size_offset(level) as u64) as u8
}

/// Writes a bind's size offset `size_offset` at `level`.
pub fn push_size_offset(stream: &mut BitStream, level: u8, size_offset: u8) {
    if size_offset == 0 {
        stream.push_value(TILE, TILE_OR_COMPLEX_WIDTH);
        return;
    }
    stream.push_value(COMPLEX, TILE_OR_COMPLEX_WIDTH);
    stream.push_truncated_binary(finest_first(level, size_offset), most_size_offset(level) as u64);
}

/// Reads what [`push_size_offset`] wrote at `level`.
pub fn read_size_offset(reader: &mut BitReader, level: u8) -> u8 {
    if reader.value(TILE_OR_COMPLEX_WIDTH) == TILE {
        return 0;
    }
    most_size_offset(level) - reader.truncated_binary(most_size_offset(level) as u64) as u8
}

/// A complex tile's size offset `size_offset` at `level` as the value
/// its truncated binary code says: the finest, `0`.
const fn finest_first(level: u8, size_offset: u8) -> u64 {
    (most_size_offset(level) - size_offset) as u64
}

/// Whether a 1x1 resolution -- and so a cell list -- can be named at
/// `level`.
pub const fn raw_resolution_fits(level: u8) -> bool {
    most_size_offset(level) == levels_to_cells(level)
}

/// The coarsest level a 1x1 resolution -- and so a cell list -- can be
/// named at: 128x128.
pub const COARSEST_RAW_LEVEL: u8 = {
    let mut level = 0;
    while !raw_resolution_fits(level) {
        level += 1;
    }
    level
};
