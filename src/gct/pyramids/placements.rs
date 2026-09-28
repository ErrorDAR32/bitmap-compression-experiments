//! What the greedy tiler placed: one 4-bit code per tile, over every
//! level down to single cells -- nothing placed exactly here, or what
//! the tile placed here is.

use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{Tile, CELL_LEVEL};

/// What a placed tile is: bound to one value, or a copy of a same-size
/// area -- a near copy of a neighbour of the tile itself, or a far copy
/// of a neighbour of its parent, at the tile's own child position --
/// in [`DIRECTIONS`](crate::gct::tile::DIRECTIONS) order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Placement {
    Bound(bool),
    Copied { far: bool, direction: u8 },
}

/// `0`: nothing placed. `1`/`2`: bound to false/true. `8..=15`: copied,
/// far in bit 2, direction in bits 0-1.
const NOTHING: u64 = 0;
/// Every placement code fits this many bits.
pub(super) const PLACEMENT_CODE_BITS: u64 = 4;
const BOUND_FALSE: u64 = 1;
const BOUND_TRUE: u64 = 2;
const COPIED: u64 = 0b1000;
const FAR: u64 = 0b100;
const DIRECTION_MASK: u64 = 0b11;

/// A placement's 4-bit code; also how the complex tiling pyramid holds it.
pub(super) fn placement_code(placement: Placement) -> u64 {
    match placement {
        Placement::Bound(false) => BOUND_FALSE,
        Placement::Bound(true) => BOUND_TRUE,
        Placement::Copied { far, direction } => COPIED | if far { FAR } else { 0 } | direction as u64,
    }
}

/// The placement a 4-bit code names, if any.
pub(super) fn placement_from_code(code: u64) -> Option<Placement> {
    match code {
        NOTHING => None,
        BOUND_FALSE => Some(Placement::Bound(false)),
        BOUND_TRUE => Some(Placement::Bound(true)),
        _ if code & COPIED != 0 => {
            Some(Placement::Copied { far: code & FAR != 0, direction: (code & DIRECTION_MASK) as u8 })
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

    /// Whether a tile was placed exactly at `tile`.
    fn is_placed(&self, tile: Tile) -> bool {
        self.placement(tile).is_some()
    }

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
