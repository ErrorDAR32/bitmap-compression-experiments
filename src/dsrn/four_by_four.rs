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
//! 10    skip
//! 11    the whole of it copied, then two bits of direction
//! then
//! 0     no child mask: the code takes all four, and skipping with
//!       no mask takes none, which leaves the whole of it to the
//!       binding above
//! 1     a child mask: four bits saying which children the code
//!       takes, then a bit
//!       0   the ones it does not take copy, each from a neighbour
//!           of its own
//!       1   the ones it does not take are left to the closest
//!           binding above
//! ```
//!
//! What the code takes it says outright. A binding writes a bit per
//! tile of it, which is one bit at 2x2 tiles and four at 1x1. A skip
//! leaves it to be a region of its own, which for a 2x2 is its four
//! cells. A copy takes it from the neighbour it named. What the code
//! does not take is either copied on its own account or already
//! right.
//!
//! Three bits say the four things a 4x4 is usually saying: all one
//! thing a child (bind at 2x2 tiles), all sixteen cells (bind at 1x1),
//! and none of my business (skip). Five say the whole of it copied.
//! The general grammar spends four before it has said anything at
//! all, and has no way to say that a 2x2 copies.

use crate::dsrn::cost::cost_of_a_child;
use crate::dsrn::describable::{
    children_standing_gets_wrong, standing_one_level_down, where_each_child_copies_from,
};
use crate::dsrn::nesting::Knobs;
use crate::dsrn::nesting_data::{
    RegionMask, Standing, Workspace, CHILD_MASK_WIDTH, DIRECTION_WIDTH,
};
use crate::dsrn::region::{same_cells, Region, CHILD_COUNT, DIRECTIONS};
use crate::pyramid::{tile_of_bitmap, Pyramid};
use crate::Bitmap;

/// The widths, in bits. Every one of them is one bit but the mask and
/// the direction, which is the point of the thing.
pub const FIRST_WIDTH: usize = 1;
pub const SECOND_WIDTH: usize = 1;
pub const TILE_SIZE_WIDTH: usize = 1;
pub const MASKED_OR_NOT_WIDTH: usize = 1;
pub const COPY_OR_LEAVE_WIDTH: usize = 1;

/// The values those bits take.
pub const BIND: u64 = 0;
pub const NOT_BIND: u64 = 1;
pub const SKIP: u64 = 0;
pub const COPY: u64 = 1;
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
pub enum TheCode {
    /// A bit per child: the one thing that child holds.
    BindAtFours,
    /// Four bits per child: its cells, one tile each.
    BindAtOnes,
    /// The child becomes a region of its own, which for a 2x2 is its
    /// four cells and no code.
    Skip,
    /// The child is taken from the same neighbour the whole region
    /// would have been.
    Copied { direction: usize },
}

impl TheCode {
    /// What one child it takes costs in payload.
    pub fn what_taking_it_costs(self) -> usize {
        match self {
            TheCode::BindAtFours => 1,
            TheCode::BindAtOnes | TheCode::Skip => CELLS_IN_A_CHILD,
            TheCode::Copied { .. } => 0,
        }
    }

    /// What the code itself costs, before any mask.
    pub fn size(self) -> usize {
        FIRST_WIDTH
            + match self {
                TheCode::BindAtFours | TheCode::BindAtOnes => TILE_SIZE_WIDTH,
                TheCode::Skip => SECOND_WIDTH,
                TheCode::Copied { .. } => SECOND_WIDTH + DIRECTION_WIDTH,
            }
    }
}

/// What becomes of a child the code does not take.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WhatItLeaves {
    /// It copies, from a neighbour of its own.
    Copies,
    /// It is left to the closest binding above, which already says
    /// what it holds.
    LeftToABinding,
}

/// What a 4x4 says.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FourByFourSays {
    pub code: TheCode,
    /// No mask means the code takes all four -- except a skip, which
    /// then takes none and leaves the whole region to the binding
    /// above.
    pub mask: Option<(RegionMask, WhatItLeaves)>,
}

impl FourByFourSays {
    /// Whether a child is one the code takes.
    pub fn takes_it(self, child: usize) -> bool {
        match self.mask {
            Some((mask, _)) => mask.describes(child),
            None => self.code != TheCode::Skip,
        }
    }

    /// What the header costs: the code, and the mask where there is
    /// one.
    pub fn header_size(self) -> usize {
        self.code.size()
            + MASKED_OR_NOT_WIDTH
            + if self.mask.is_some() { CHILD_MASK_WIDTH + COPY_OR_LEAVE_WIDTH } else { 0 }
    }
}

/// Everything a 4x4 could say, and what each would cost all told.
///
/// Each way is priced child by child, because which children a mask
/// names is settled one child at a time: a child goes wherever it is
/// cheaper, and if it can neither be taken nor left, that way of
/// saying it is not open at all.
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
    let one_thing = children
        .map(|child| tile_of_bitmap(pyramid, bitmap, child.level, child.x, child.y).is_some());
    let below = standing_one_level_down(work, standing, region);

    let mut codes = vec![TheCode::BindAtFours, TheCode::BindAtOnes, TheCode::Skip];
    if pyramid.copyable(region.level, region.x, region.y) {
        codes.extend(
            (0..DIRECTIONS.len())
                .filter(|&direction| a_neighbour_worth_copying(work, bitmap, region, direction, encoded))
                .map(|direction| TheCode::Copied { direction }),
        );
    }

    for code in codes {
        let may_take = |at: usize| match code {
            TheCode::BindAtFours => one_thing[at],
            TheCode::Copied { direction } => {
                children_the_copy_holds(work, bitmap, region, direction, encoded).describes(at)
            }
            _ => true,
        };
        let taking = code.what_taking_it_costs();

        // No mask at all. A skip then takes nothing, and the whole
        // region is left to the binding above, which only works where
        // the whole of it is already right.
        let takes_none = code == TheCode::Skip;
        let open = if takes_none {
            wrong == RegionMask::NONE
        } else {
            (0..CHILD_COUNT).all(may_take)
        };
        if open {
            let says = FourByFourSays { code, mask: None };
            let left = if takes_none {
                (0..CHILD_COUNT)
                    .map(|at| cost_of_a_child(work, children[at], below, false))
                    .sum()
            } else {
                CHILD_COUNT * taking
            };
            ways.push((says, says.header_size() + left));
        }

        for leaves in [WhatItLeaves::Copies, WhatItLeaves::LeftToABinding] {
            let (mut mask, mut size) = (0u64, 0usize);
            let mut open = true;
            for (at, child) in children.into_iter().enumerate() {
                let left = match leaves {
                    WhatItLeaves::Copies => from[at].map(|_| DIRECTION_WIDTH),
                    WhatItLeaves::LeftToABinding => {
                        (!wrong.describes(at)).then(|| cost_of_a_child(work, child, below, false))
                    }
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
            let says = FourByFourSays { code, mask: Some((RegionMask(mask), leaves)) };
            if open {
                ways.push((says, says.header_size() + size));
            }
        }
    }
    ways
}

/// Which children of a region a copy in a direction would hold right.
fn children_the_copy_holds(
    work: &Workspace,
    bitmap: &Bitmap,
    region: Region,
    direction: usize,
    encoded: bool,
) -> RegionMask {
    let Some(from) = region.neighbour(direction) else { return RegionMask::NONE };
    let theirs = from.children();
    let mut held = 0;
    for (at, mine) in region.children().into_iter().enumerate() {
        if same_cells(bitmap, mine, theirs[at])
            && (!encoded || work.region_taken.whole_region_taken(theirs[at]))
        {
            held |= 1 << at;
        }
    }
    RegionMask(held)
}

/// Whether a direction is worth naming at all: the neighbour is on
/// the bitmap and holds at least one of this region's children.
fn a_neighbour_worth_copying(
    work: &Workspace,
    bitmap: &Bitmap,
    region: Region,
    direction: usize,
    encoded: bool,
) -> bool {
    children_the_copy_holds(work, bitmap, region, direction, encoded) != RegionMask::NONE
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
        .expect("a 4x4 can always bind at one cell a tile, which takes every child")
}
