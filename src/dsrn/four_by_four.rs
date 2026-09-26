//! The grammar a 4x4 gets instead of the one every other region has.
//!
//! A 4x4 is the last region with anything to choose. Its children are
//! 2x2s, which have no grammar at all -- they write their four cells
//! and say nothing -- so everything that can be said about a 2x2 has
//! to be said here, and everything said here is said about four of
//! them at once. That is a different job from the one the general
//! grammar does, and this is a different grammar for it.
//!
//! ```text
//! 0     bind, then a bit: 2x2 tiles or 1x1
//! 1     skip
//! then
//! 0     unmasked: the code takes all four children
//! 1     masked: four bits saying which it takes, then a bit
//!       0   the ones it does not take copy, each from a neighbour
//!           of its own
//!       1   the ones it does not take are left to the closest
//!           binding above
//! ```
//!
//! What the code takes it says outright: a binding writes a bit per
//! tile of it, and a skip leaves it to be a region of its own, which
//! for a 2x2 means its four cells. What the code does not take is
//! either copied or already right.
//!
//! There are three readings that carry no mask and four things
//! wanting one, so one of them has to go. The one dropped is skipping
//! and taking nothing -- a 4x4 saying that the whole of it is already
//! right -- because a region is only described at all when the region
//! above it named it, and a region above only names a child that is
//! not already right. Nothing can reach it, so it is free.
//!
//! What it says instead is the one thing the rest of this grammar
//! cannot: the whole 4x4 copied from one neighbour, in five bits
//! rather than the fifteen it would cost to say it four times over.
//! Skipping and taking all four writes the four children out raw, and
//! is a bit cheaper for it.
//!
//! ```text
//! 1 0                                the four children, raw
//! 0 1 0 then two bits of direction   the whole of it, copied
//! ```
//!
//! Two bits buy what four used to. The general grammar spends two on
//! the code and two more on the tile size to say what this says in
//! two altogether, and it has no way at all to say that a 2x2 copies.
//! What it still says for one bit less is a 4x4 bound at one tile,
//! which here costs three bits and four rather than three and one.

use crate::dsrn::cost::cost_of_a_child;
use crate::dsrn::describable::{children_standing_gets_wrong, where_each_child_copies_from};
use crate::dsrn::nesting::Knobs;
use crate::dsrn::nesting_data::{RegionMask, Standing, Workspace, CHILD_MASK_WIDTH};
use crate::dsrn::region::{Region, CHILD_COUNT};
use crate::pyramid::{tile_of_bitmap, Pyramid};
use crate::Bitmap;

/// The widths, in bits. Every one of them is one bit, which is the
/// point of the thing.
pub const BIND_OR_SKIP_WIDTH: usize = 1;
pub const TILE_SIZE_WIDTH: usize = 1;
pub const MASKED_OR_NOT_WIDTH: usize = 1;
pub const COPY_OR_LEAVE_WIDTH: usize = 1;

/// The values those bits take.
pub const BIND: u64 = 0;
pub const SKIP: u64 = 1;
pub const TILES_OF_FOUR: u64 = 0;
pub const TILES_OF_ONE: u64 = 1;
pub const UNMASKED: u64 = 0;
pub const MASKED: u64 = 1;
pub const THE_REST_COPY: u64 = 0;
pub const THE_REST_ARE_LEFT: u64 = 1;

/// Cells in a child of a 4x4, which is the four of a 2x2.
pub const CELLS_IN_A_CHILD: usize = 4;

/// What the code does with a child it takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WhatItTakes {
    /// A bit per child: the one thing that child holds.
    BindAtFours,
    /// Four bits per child: its cells, one tile each.
    BindAtOnes,
    /// The child becomes a region of its own, which for a 2x2 is its
    /// four cells and no code.
    Skip,
}

/// What becomes of a child the code does not take.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WhatItLeaves {
    /// Nothing: there is no mask, and the code takes all four.
    Nothing,
    /// It copies, from a neighbour of its own.
    Copies,
    /// It is left to the closest binding above, which already says
    /// what it holds.
    LeftToABinding,
}

/// What a 4x4 says.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FourByFourSays {
    /// The whole of it, taken from one neighbour. Written in the one
    /// reading of the code that would otherwise say what skipping
    /// says for a bit less.
    CopiedWhole { direction: usize },
    /// What the code takes, and what becomes of the rest.
    Said { takes: WhatItTakes, mask: RegionMask, leaves: WhatItLeaves },
}

impl FourByFourSays {
    /// Whether a child is one the code takes.
    pub fn takes_it(self, child: usize) -> bool {
        match self {
            FourByFourSays::CopiedWhole { .. } => true,
            FourByFourSays::Said { mask, leaves, .. } => {
                leaves == WhatItLeaves::Nothing || mask.describes(child)
            }
        }
    }

    /// What the header costs: the code, and the mask where there is
    /// one.
    pub fn header_size(self) -> usize {
        let code = BIND_OR_SKIP_WIDTH + MASKED_OR_NOT_WIDTH;
        match self {
            FourByFourSays::CopiedWhole { .. } => {
                code + TILE_SIZE_WIDTH + crate::dsrn::nesting_data::DIRECTION_WIDTH
            }
            FourByFourSays::Said { takes, leaves, .. } => {
                let code = code + usize::from(takes != WhatItTakes::Skip) * TILE_SIZE_WIDTH;
                if leaves == WhatItLeaves::Nothing {
                    code
                } else {
                    code + CHILD_MASK_WIDTH + COPY_OR_LEAVE_WIDTH
                }
            }
        }
    }
}

/// What one child costs the 4x4 that took it.
fn what_taking_it_costs(takes: WhatItTakes) -> usize {
    match takes {
        WhatItTakes::BindAtFours => 1,
        WhatItTakes::BindAtOnes | WhatItTakes::Skip => CELLS_IN_A_CHILD,
    }
}

/// Everything a 4x4 could say, and what each would cost all told.
///
/// The three ways of taking a child and the two ways of leaving one
/// make six, and then there is taking all four and leaving none,
/// which makes nine. Each is priced child by child, because which
/// children a mask names is settled one child at a time: a child goes
/// wherever it is cheaper, and if it cannot go where the code takes
/// it, it has to be left, and if it cannot be left either, that way
/// of saying it is not open.
pub fn every_way_a_four_by_four_can_say_it(
    work: &Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    knobs: Knobs,
    encoded: bool,
    standing: Standing,
) -> Vec<(FourByFourSays, usize)> {
    let mut ways = Vec::new();
    if !knobs.four_by_four.is_its_own_grammar(region) {
        return ways;
    }
    let wrong = children_standing_gets_wrong(work, pyramid, bitmap, region, standing);
    let from = where_each_child_copies_from(work, pyramid, bitmap, region, encoded);
    let children = region.children();
    let one_thing = children.map(|child| {
        tile_of_bitmap(pyramid, bitmap, child.level, child.x, child.y).is_some()
    });
    let below = crate::dsrn::describable::standing_one_level_down(work, standing, region);

    // The whole of it from one neighbour, where there is one to take
    // it from.
    if pyramid.copyable(region.level, region.x, region.y) {
        if let Some(direction) = a_neighbour_holding_the_same(work, bitmap, region, encoded) {
            let says = FourByFourSays::CopiedWhole { direction };
            ways.push((says, says.header_size()));
        }
    }

    for takes in [WhatItTakes::BindAtFours, WhatItTakes::BindAtOnes, WhatItTakes::Skip] {
        // A binding at one tile a child can only take a child that is
        // all one thing, or the bit it writes for it would be a lie.
        let may_take = |at: usize| takes != WhatItTakes::BindAtFours || one_thing[at];
        let taking = what_taking_it_costs(takes);

        // Taking all four at one tile a cell is the reading that now
        // says copy, so it is not one of these. Skipping and taking
        // all four writes them raw, which is what it would have
        // written anyway, a bit cheaper.
        if takes != WhatItTakes::BindAtOnes && (0..CHILD_COUNT).all(may_take) {
            let says = FourByFourSays::Said {
                takes,
                mask: RegionMask::EVERY,
                leaves: WhatItLeaves::Nothing,
            };
            ways.push((says, says.header_size() + CHILD_COUNT * taking));
        }

        for leaves in [WhatItLeaves::Copies, WhatItLeaves::LeftToABinding] {
            let (mut mask, mut size) = (0u64, 0usize);
            let mut open = true;
            for (at, child) in children.into_iter().enumerate() {
                // What it costs to be left rather than taken, where
                // being left is even allowed.
                let left = match leaves {
                    WhatItLeaves::Copies => from[at].map(|_| crate::dsrn::nesting_data::DIRECTION_WIDTH),
                    WhatItLeaves::LeftToABinding => (!wrong.describes(at))
                        .then(|| cost_of_a_child(work, child, below, false)),
                    WhatItLeaves::Nothing => None,
                };
                match (may_take(at).then_some(taking), left) {
                    (Some(taken), Some(left)) if left < taken => size += left,
                    (Some(taken), _) => {
                        mask |= 1 << at;
                        size += taken;
                    }
                    (None, Some(left)) => size += left,
                    (None, None) => open = false,
                }
            }
            let says = FourByFourSays::Said { takes, mask: RegionMask(mask), leaves };
            if open && RegionMask(mask) != RegionMask::EVERY {
                ways.push((says, says.header_size() + size));
            }
        }
    }
    ways
}

/// A neighbour of the region's own size holding the same cells, and
/// one the decoder will be holding by the time it is asked for.
fn a_neighbour_holding_the_same(
    work: &Workspace,
    bitmap: &Bitmap,
    region: Region,
    encoded: bool,
) -> Option<usize> {
    (0..crate::dsrn::region::DIRECTIONS.len()).find(|&direction| {
        region.neighbour(direction).is_some_and(|from| {
            crate::dsrn::region::same_cells(bitmap, region, from)
                && (!encoded
                    || crate::dsrn::region::whole_region_encoded(&work.encoded_cells, from))
        })
    })
}

/// The cheapest of them.
pub fn what_a_four_by_four_says(
    work: &Workspace,
    pyramid: &Pyramid,
    bitmap: &Bitmap,
    region: Region,
    knobs: Knobs,
    encoded: bool,
    standing: Standing,
) -> (FourByFourSays, usize) {
    every_way_a_four_by_four_can_say_it(work, pyramid, bitmap, region, knobs, encoded, standing)
        .into_iter()
        .min_by_key(|&(_, size)| size)
        .expect("a 4x4 can always skip, which takes every child at four bits")
}
