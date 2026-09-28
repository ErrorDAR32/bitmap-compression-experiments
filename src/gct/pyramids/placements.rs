//! What the greedy tiler placed: one 8-bit code per tile, over every
//! level down to single cells, held in the complex tiling pyramid -- nothing placed exactly here, or what
//! the tile placed here is.
//!
//! A copy or a bind may mask some of its children: it says only the
//! others, and each child it masks is left to tiles placed inside that
//! child.

use crate::gct::grammar::{DIRECTION_MASK, DIRECTION_WIDTH, FAR_WIDTH};
use crate::gct::tile::{Tile, ALL_CHILDREN, CELL_LEVEL, CHILDREN};

/// What a placed tile is: bound to one value, or a copy of a same-size
/// area -- a near copy of a neighbour of the tile itself, or a far copy
/// of a neighbour of its parent, at the tile's own child position --
/// in [`DIRECTIONS`](crate::gct::tile::DIRECTIONS) order.
///
/// Either may mask some of its children: `masked_children` has bit `i`
/// set for each child, `i` in reading order, it leaves to the tiles
/// placed inside that child; `0` when it says the whole tile. A bind
/// that masks binds every child it leaves unnamed, at any depth:
/// whatever inside it
/// nothing is placed at says its value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Placement {
    /// Bound to one value.
    Bound {
        /// The value it binds.
        value: bool,
        /// The children it masks, bit `i` for child `i`.
        masked_children: u8,
    },
    /// A copy of a same-size area.
    Copied {
        /// Whether it copies a neighbour of its parent rather than of
        /// itself.
        far: bool,
        /// Which of [`DIRECTIONS`](crate::gct::tile::DIRECTIONS) it
        /// copies from.
        direction: u8,
        /// The children it masks, bit `i` for child `i`.
        masked_children: u8,
    },
}

impl Placement {
    /// A bind of the whole tile.
    pub const fn bound(value: bool) -> Self {
        Placement::Bound { value, masked_children: 0 }
    }

    /// The children it masks, bit `i` for child `i`.
    fn masked_children(self) -> u8 {
        match self {
            Placement::Bound { masked_children, .. } | Placement::Copied { masked_children, .. } => masked_children,
        }
    }

    /// Whether this masks any of its tile's children.
    pub fn masks_any(self) -> bool {
        self.masked_children() != 0
    }

    /// Whether this, placed at `child`'s parent, masks `child`: leaves
    /// it to the tiles placed inside it.
    pub fn masks(self, child: Tile) -> bool {
        self.masked_children() & (1 << child.child_index()) != 0
    }

    /// Whether this is a bind of the whole tile: what a complex tile can
    /// unmask.
    pub fn is_whole_bind(self) -> bool {
        matches!(self, Placement::Bound { masked_children: 0, .. })
    }
}

/// The value bound at the top of the bitmap, before any bind that
/// masks: clear.
pub const BOUND_AT_THE_TOP: bool = false;

/// The finest level a placed tile masks at: 8x8. Masking a 4x4 never
/// pays -- its children, 2x2s, cost at most 2 bits each, less than the
/// 4-bit child mask saves. Measured (`docs/gct.md`): allowing copies to
/// costs every family bits on every seed, blob about 6.6%.
pub const FINEST_MASKING_LEVEL: u8 = CELL_LEVEL - 3;

/// A placement code's bits 0-3 for nothing placed: `0`, so an all-zero
/// element holds no placement. Bits 0-3 are otherwise `1`/`2` bound to
/// false/true, `8..=15` copied; bits 4-7 the children it masks.
const NOTHING: u64 = 0;
/// The kind of a bind to false.
const BOUND_FALSE: u64 = 1;
/// The kind of a bind to true.
const BOUND_TRUE: u64 = 2;
/// Set in a copy's kind when it is far: the bit above its direction's.
const FAR: u64 = 1 << DIRECTION_WIDTH;
/// Set in the kind of every copy: the bit above far.
const COPIED: u64 = FAR << FAR_WIDTH;
/// Bits a kind takes: a copy's direction, far, and the copy bit.
const KIND_BITS: u64 = COPIED.trailing_zeros() as u64 + 1;
/// A kind's bits, at the bottom of a code.
const KIND_MASK: u64 = (1 << KIND_BITS) - 1;
/// Where the masked children start: after the kind.
const MASKED_CHILDREN_SHIFT: u64 = KIND_BITS;
/// Every placement code fits this many bits: the kind, then a bit a
/// child.
pub(super) const PLACEMENT_CODE_BITS: u64 = KIND_BITS + CHILDREN as u64;

/// A placement's code; also how the complex tiling pyramid holds it.
pub(super) fn placement_code(placement: Placement) -> u64 {
    let kind = match placement {
        Placement::Bound { value: false, .. } => BOUND_FALSE,
        Placement::Bound { value: true, .. } => BOUND_TRUE,
        Placement::Copied { far, direction, .. } => COPIED | if far { FAR } else { 0 } | direction as u64,
    };
    kind | (placement.masked_children() as u64) << MASKED_CHILDREN_SHIFT
}

/// The placement a code names, if any.
pub(super) fn placement_from_code(code: u64) -> Option<Placement> {
    let masked_children = ((code >> MASKED_CHILDREN_SHIFT) & ALL_CHILDREN as u64) as u8;
    match code & KIND_MASK {
        NOTHING => None,
        BOUND_FALSE => Some(Placement::Bound { value: false, masked_children }),
        BOUND_TRUE => Some(Placement::Bound { value: true, masked_children }),
        kind if kind & COPIED != 0 => {
            Some(Placement::Copied { far: kind & FAR != 0, direction: (kind & DIRECTION_MASK) as u8, masked_children })
        }
        _ => unreachable!("no placement has code {code}"),
    }
}

/// What the greedy tiler placed, held in the placement bits of the
/// complex tiling pyramid ([`super::complex_tiling`]), whose other bits
/// the complex tiler fills in later.
pub trait Placements {
    /// Nothing placed anywhere yet.
    fn placements() -> Self;

    /// The tile placed exactly at `tile`, if any.
    fn placement(&self, tile: Tile) -> Option<Placement>;

    /// Records `placement` as placed exactly at `tile`.
    fn place(&mut self, tile: Tile, placement: Placement);

    /// Every placed tile, coarsest level first, Morton order within
    /// each level.
    fn placed_tiles(&self) -> impl Iterator<Item = (Tile, Placement)> + '_;
}
