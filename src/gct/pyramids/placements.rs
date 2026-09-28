//! What the greedy tiler placed: one 8-bit code per tile, over every
//! level down to single cells -- nothing placed exactly here, or what
//! the tile placed here is.
//!
//! A copy or a bind may mask some of its children: it says only the
//! others, and each child it masks is left to tiles placed inside that
//! child.

use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{Tile, CELL_LEVEL};

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
    Bound { value: bool, masked_children: u8 },
    Copied { far: bool, direction: u8, masked_children: u8 },
}

impl Placement {
    /// A bind of the whole tile.
    pub const fn bound(value: bool) -> Self {
        Placement::Bound { value, masked_children: 0 }
    }

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

/// The value bound above `tile`: the value of the nearest bind that
/// masks above it, or clear -- `placed_at` telling what is placed where.
pub fn binding_above(tile: Tile, placed_at: impl Fn(Tile) -> Option<Placement>) -> bool {
    (0..tile.level)
        .rev()
        .find_map(|level| match placed_at(tile.ancestor(level)) {
            Some(Placement::Bound { value, masked_children }) if masked_children != 0 => Some(value),
            _ => None,
        })
        .unwrap_or(BOUND_AT_THE_TOP)
}

/// The finest level a placed tile masks at: 8x8. Masking a 4x4 never
/// pays -- its children, 2x2s, cost at most 2 bits each, less than the
/// 4-bit child mask saves. Measured (`docs/gct.md`): allowing copies to
/// costs every family bits on every seed, blob about 6.6%.
pub const FINEST_MASKING_LEVEL: u8 = CELL_LEVEL - 3;

/// Bits 0-3: `0` nothing placed, `1`/`2` bound to false/true, `8..=15`
/// copied, far in bit 2 and direction in bits 0-1. Bits 4-7: the
/// children it masks.
const NOTHING: u64 = 0;
/// Every placement code fits this many bits.
pub(super) const PLACEMENT_CODE_BITS: u64 = 8;
const KIND_MASK: u64 = 0b1111;
const BOUND_FALSE: u64 = 1;
const BOUND_TRUE: u64 = 2;
const COPIED: u64 = 0b1000;
const FAR: u64 = 0b100;
const DIRECTION_MASK: u64 = 0b11;
const MASKED_CHILDREN_SHIFT: u64 = 4;
const MASKED_CHILDREN_MASK: u64 = 0b1111;

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
    let masked_children = ((code >> MASKED_CHILDREN_SHIFT) & MASKED_CHILDREN_MASK) as u8;
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

const SHAPE: PyramidShape =
    PyramidShape { arity: 4, coarsest_level: 0, finest_level: CELL_LEVEL, element_bits: PLACEMENT_CODE_BITS as usize };

pub trait Placements {
    /// Nothing placed anywhere yet.
    fn placements() -> Self;

    /// The tile placed exactly at `tile`, if any.
    fn placement(&self, tile: Tile) -> Option<Placement>;

    fn place(&mut self, tile: Tile, placement: Placement);

    /// Every placed tile, coarsest level first, reading order within
    /// each level.
    fn placed_tiles(&self) -> impl Iterator<Item = (Tile, Placement)> + '_;
}

impl Placements for Pyramid {
    fn placements() -> Self {
        Pyramid::new(SHAPE)
    }

    fn placement(&self, tile: Tile) -> Option<Placement> {
        placement_from_code(self.get(tile))
    }

    fn place(&mut self, tile: Tile, placement: Placement) {
        self.set(tile, placement_code(placement));
    }

    fn placed_tiles(&self) -> impl Iterator<Item = (Tile, Placement)> + '_ {
        (0..=CELL_LEVEL).flat_map(move |level| {
            self.tiles_of_level(level).filter_map(move |tile| self.placement(tile).map(|placement| (tile, placement)))
        })
    }
}
