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
//!                 children are described again over the top of it
//! ```
//!
//! The fourth is not a fourth thing to say. It is the other three said
//! of part of a region, and the point of it being a code rather than a
//! flag on a binding is that nothing pays for a mask it does not use.
//!
//! A description acts over the whole of its region. A binding covers
//! every cell of it, a copy takes every cell of it. What a mask adds
//! is an override: a child named by it is described again, takes its
//! own cells, and the binding above writes no bit for a tile that
//! falls inside it. A child not named is left to the closest binding
//! above, which is what covers it.
//!
//! A region finer than [`FINEST_LEVEL_WITH_A_GRAMMAR`] writes its
//! cells and no code at all.

use crate::dsrn::coarsest::coarsest;
use crate::dsrn::encode::encode_region;
use crate::dsrn::nesting_data::{Encoded, Standing, Workspace, FINEST_LEVEL_WITH_A_GRAMMAR};
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

/// What the finest region with a grammar does with itself.
///
/// Masking never pays below a 4x4, which leaves the mask code unused
/// there and raises the question of whether a 4x4 needs a code at
/// all. [`FourByFour::AlwaysMasks`] says it does not: it is bound by
/// definition, writes a four bit mask, and each of its 2x2 children
/// is either a direction to copy from or its four cells written out.
///
/// The decoder has to know which of these was used, because they are
/// different grammars rather than different choices within one.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FourByFour {
    /// The same grammar as any other region: a code, and everything
    /// that code can say.
    LikeAnyRegion,
    /// That, and one thing more, written in the one tile size its
    /// field has no size for: its four children each copy from a
    /// neighbour of their own, but for the ones already right, which
    /// are left to the binding above.
    ///
    /// A 2x2 has no grammar and so can never say copy. This is the
    /// only place it can be said for it, and it is said in a field
    /// that was being paid for anyway.
    AlsoCopiesEachChild,
    /// No code. A mask, and a direction or four cells per child.
    AlwaysMasks,
    /// A grammar of its own, one bit a choice: bind or skip, at which
    /// of two tile sizes, masked or not, and what becomes of the
    /// children the mask leaves. [`super::four_by_four`] has it.
    ItsOwnGrammar,
}

impl FourByFour {
    pub const ALL: [FourByFour; 4] = [
        FourByFour::LikeAnyRegion,
        FourByFour::AlsoCopiesEachChild,
        FourByFour::AlwaysMasks,
        FourByFour::ItsOwnGrammar,
    ];

    pub fn name(self) -> &'static str {
        match self {
            FourByFour::LikeAnyRegion => "a 4x4 says what any region says",
            FourByFour::AlsoCopiesEachChild => "a 4x4 may say its children copy themselves",
            FourByFour::AlwaysMasks => "a 4x4 always masks its four children",
            FourByFour::ItsOwnGrammar => "a 4x4 has a grammar of its own",
        }
    }

    /// Whether this region is one the rule is about.
    pub fn applies_to(self, region: Region) -> bool {
        self == FourByFour::AlwaysMasks && region.level == FINEST_LEVEL_WITH_A_GRAMMAR
    }

    /// Whether this region may say that its children copy themselves.
    pub fn may_copy_each_child(self, region: Region) -> bool {
        self == FourByFour::AlsoCopiesEachChild
            && region.level == FINEST_LEVEL_WITH_A_GRAMMAR
    }

    /// Whether this region says it in a grammar of its own.
    pub fn is_its_own_grammar(self, region: Region) -> bool {
        self == FourByFour::ItsOwnGrammar && region.level == FINEST_LEVEL_WITH_A_GRAMMAR
    }
}

/// Everything about an encode that is a choice rather than the
/// algorithm.
///
/// One value rather than a parameter each, so that adding a knob is a
/// field and not a change to every signature between here and where
/// it is read.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Knobs {
    pub masking: Masking,
    pub four_by_four: FourByFour,
}

impl Default for Knobs {
    fn default() -> Self {
        Self { masking: Masking::Anywhere, four_by_four: FourByFour::LikeAnyRegion }
    }
}

impl Knobs {
    pub fn name(self) -> String {
        format!("masking {}, and {}", self.masking.name(), self.four_by_four.name())
    }
}

/// Encodes the bitmap. The pyramid must already hold it.
pub fn encode(
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    knobs: Knobs,
    work: &mut Workspace,
    out: &mut Encoded,
) {
    out.clear();
    work.encoded_cells.reset();
    work.region_taken.reset();
    let whole = Region::whole_bitmap();
    coarsest(work, pyramid, bitmap, whole, knobs);
    encode_region(work, pyramid, bitmap, whole, knobs, Standing::Clear, out);
}
