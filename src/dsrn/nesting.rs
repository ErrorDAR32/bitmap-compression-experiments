//! Disjoint sized-tile region nesting: the entry point and its knobs.
//!
//! This file is the shape of the thing and nothing else. What a region
//! can say is [`describable`](super::describable), what it costs is
//! [`cost`](super::cost), the pass that prices everything is
//! [`coarsest`](super::coarsest), writing it is
//! [`encode`](super::encode) and reading it back is
//! [`decode`](super::decode).
//!
//! # The codes
//!
//! Every region writes two bits. The decoder always knows which region
//! it stands on, so nothing says where it is or how big.
//!
//! ```text
//! 00  bind        a tile size, then a payload bit for every tile
//! 01  subdivide   the four children, in reading order
//! 10  copy        a direction
//! 11  mask        one of the three above, then four bits saying which
//!                 children it covers and which become regions
//! ```
//!
//! The fourth is not a fourth thing to say. It is the other three said
//! of part of a region, and the point of it being a code rather than a
//! flag on a binding is that nothing pays for a mask it does not use.
//!
//! A region finer than [`FINEST_LEVEL_WITH_A_GRAMMAR`] writes its
//! cells and no code at all.

use crate::dsrn::coarsest::coarsest;
use crate::dsrn::encode::encode_region;
use crate::dsrn::nesting_data::{Encoded, Workspace, FINEST_LEVEL_WITH_A_GRAMMAR};
use crate::dsrn::region::Region;
use crate::pyramid::{tile_side, Pyramid, CELL_LEVEL};
use crate::Bitmap;

/// The coarsest region allowed to mask.
///
/// Masking is only ever paid for where it is used, so forbidding it
/// cannot make an encoding smaller by itself -- it can only take an
/// option away from a region that would have chosen it. What it can
/// change is everything above, because a region's cost is what its
/// children cost.
///
/// Nothing in the stream says which of these was used. A mask is a
/// code the decoder reads when it finds it, so this is a rule the
/// encoder keeps to and the decoder never needs to know.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Masking {
    /// Any region with a grammar at all.
    Anywhere,
    From4,
    From8,
    From16,
}

impl Masking {
    pub const ALL: [Masking; 4] =
        [Masking::Anywhere, Masking::From4, Masking::From8, Masking::From16];

    /// The side of the smallest region that may mask, in cells.
    pub fn smallest_side(self) -> usize {
        tile_side(self.finest_level())
    }

    /// The finest level that may mask.
    fn finest_level(self) -> usize {
        match self {
            Masking::Anywhere => CELL_LEVEL - 1,
            Masking::From4 => CELL_LEVEL - 2,
            Masking::From8 => CELL_LEVEL - 3,
            Masking::From16 => CELL_LEVEL - 4,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Masking::Anywhere => "anywhere",
            Masking::From4 => "4x4 and larger",
            Masking::From8 => "8x8 and larger",
            Masking::From16 => "16x16 and larger",
        }
    }

    /// Whether a region is allowed to mask.
    pub fn allows(self, region: Region) -> bool {
        region.level <= self.finest_level()
    }
}

/// Encodes the bitmap. The pyramid must already hold it.
pub fn encode(
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    masking: Masking,
    work: &mut Workspace,
    out: &mut Encoded,
) {
    out.clear();
    work.encoded_cells.reset();
    let whole = Region::whole_bitmap();
    coarsest(work, pyramid, bitmap, whole, masking);
    encode_region(work, pyramid, bitmap, whole, masking, out);
    let _ = FINEST_LEVEL_WITH_A_GRAMMAR;
}
