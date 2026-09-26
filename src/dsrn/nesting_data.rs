//! What an encode says, what it produces, and the room it works in.
//!
//! Nothing here does anything. What to say about a region is
//! [`super::coarsest`] and [`super::encode`]; how to say it is
//! [`super::encode`]; how much it costs is [`super::cost`]. This is
//! the vocabulary all three share.

use crate::dsrn::region::{Region, CHILD_COUNT, EVERY_CHILD};
use crate::dsrn::stream::EncodedBitmap;
use crate::pyramid::{tiles_in_level, CELL_LEVEL};
use crate::Bitmap;

/// The two bit code every region writes.
pub const BIND: u64 = 0b00;
pub const SUBDIVIDE: u64 = 0b01;
pub const COPY: u64 = 0b10;
/// Not a fourth thing to say: a mark that one of the three above is
/// about to be said of part of the region.
pub const MASK: u64 = 0b11;

/// The widths, in bits.
pub const CODE_WIDTH: usize = 2;
pub const DIRECTION_WIDTH: usize = 2;
pub const CHILD_MASK_WIDTH: usize = 4;

/// The finest region with a grammar of its own. Below it, a region
/// writes its cells and no code at all.
///
/// A 2x2 can only say four things, and three of them cost the same.
/// All one thing is a code, a tile size and a value: four bits. A copy
/// is a code and a direction: four bits. Subdividing is a code and
/// four cells that each cost a code of their own, which is never worth
/// it. Its four cells written out are four bits and no code, so it
/// ties the two that were any good and saves three on the one that was
/// not. A cell is the same argument at its limit: the only thing it
/// can say is its value, so it says its value and nothing else.
pub const FINEST_LEVEL_WITH_A_GRAMMAR: usize = CELL_LEVEL - 2;

/// Which children a description covers, and which become regions of
/// their own.
///
/// A bit set names a child the operation handles itself; a bit clear
/// names one that is described separately afterwards. All four set is
/// the whole region and needs no mask to say so -- except for
/// subdividing, which covers nothing by definition, so for it none
/// set is the unmasked case.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RegionMask(pub u64);

impl RegionMask {
    pub const EVERY: Self = Self(EVERY_CHILD);
    pub const NONE: Self = Self(0);

    /// Whether the mask names a child.
    pub fn covers(self, child: usize) -> bool {
        self.0 >> child & 1 == 1
    }

    /// How many children it names.
    pub fn covered(self) -> usize {
        self.0.count_ones() as usize
    }

    /// How many it leaves to describe themselves.
    pub fn left_to_describe(self) -> usize {
        CHILD_COUNT - self.covered()
    }
}

/// What a region says about itself, and the mask that goes with it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RegionCode {
    /// Bound at a tile size `depth` levels below the region.
    Bind { level: usize, depth: usize, mask: RegionMask },
    /// Left to the children the mask does not cover; the ones it does
    /// stay clear.
    Subdivide { mask: RegionMask },
    /// Taken from the neighbour in a direction.
    Copy { direction: usize, mask: RegionMask },
}

impl RegionCode {
    pub fn mask(self) -> RegionMask {
        match self {
            RegionCode::Bind { mask, .. }
            | RegionCode::Subdivide { mask }
            | RegionCode::Copy { mask, .. } => mask,
        }
    }

    /// Whether it needs the mark and the mask written out.
    ///
    /// Subdividing is masked whenever it covers any child, because
    /// subdividing without a mask covers none; the other two are
    /// masked whenever they do not cover all four.
    pub fn is_masked(self) -> bool {
        match self {
            RegionCode::Subdivide { mask } => mask != RegionMask::NONE,
            _ => self.mask() != RegionMask::EVERY,
        }
    }
}

/// How many of each code an encode wrote.
#[derive(Default, Clone, Copy)]
pub struct CodeCounts {
    pub bindings: usize,
    pub subdivides: usize,
    pub copies: usize,
    /// Of each of those, the ones that masked.
    pub masked_bindings: usize,
    pub masked_subdivides: usize,
    pub masked_copies: usize,
    /// Children a mask sent off to be regions of their own, and
    /// children a masked subdivision left clear.
    pub children_made_regions: usize,
    pub children_left_clear: usize,
    /// Regions below the grammar, which wrote their cells and no code.
    pub below_the_grammar: usize,
    /// 4x4s that masked their four children by definition, and how
    /// many of those children were copied rather than written out.
    pub four_by_four_masks: usize,
    pub children_copied: usize,
    /// Regions bound at one cell a tile, and the payload bits that
    /// went out one cell at a time -- the encoding at its floor.
    pub bound_at_cells: usize,
    pub cells_written: usize,
    /// Those cells, by the level of the region that gave up.
    pub cells_given_up: [usize; CELL_LEVEL + 1],
    /// Of those regions, the ones holding the same cells as a
    /// neighbour they may copy from -- but one the decoder will not
    /// hold by the time it arrives -- and the ones with no matching
    /// neighbour at all.
    pub copies_just_missed: usize,
    pub no_neighbour_matched: usize,
    /// Every bit this encode believes it wrote, added up one region at
    /// a time from the mask arithmetic rather than from the loops that
    /// write them. If it is not [`Encoded::bits`], one of the two is
    /// wrong.
    pub accounted: usize,
}

/// What an encode produces.
#[derive(Default)]
pub struct Encoded {
    pub counts: CodeCounts,
    /// The codes, tile sizes, child masks and directions.
    pub tree: EncodedBitmap,
    /// One bit per tile, in the order the regions bound them.
    pub payload: EncodedBitmap,
}

impl Encoded {
    /// Every bit of the encoding.
    pub fn bits(&self) -> usize {
        self.tree.len() + self.payload.len()
    }

    pub fn clear(&mut self) {
        self.counts = CodeCounts::default();
        self.tree.clear();
        self.payload.clear();
    }
}

/// The room an encode works in. One entry per region of every level,
/// found once and refilled per bitmap.
pub struct Workspace {
    /// Per region, the coarsest tile size at which every tile of it is
    /// homogeneous, as a depth below the region.
    pub coarsest_homogeneous_depth: Vec<Vec<u8>>,
    /// Per region, what its cheapest description costs.
    pub cheapest_description: Vec<Vec<usize>>,
    /// Which cells the descent has described so far, which is what the
    /// decoder will hold when it arrives.
    pub encoded_cells: Bitmap,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            coarsest_homogeneous_depth: (0..=CELL_LEVEL)
                .map(|level| vec![0u8; tiles_in_level(level)])
                .collect(),
            cheapest_description: (0..=CELL_LEVEL)
                .map(|level| vec![0usize; tiles_in_level(level)])
                .collect(),
            encoded_cells: Bitmap::new(),
        }
    }

    fn at(region: Region) -> usize {
        region.y * crate::pyramid::tiles_across(region.level) + region.x
    }

    pub fn coarsest_depth_of(&self, region: Region) -> usize {
        self.coarsest_homogeneous_depth[region.level][Self::at(region)] as usize
    }

    pub fn set_coarsest_depth(&mut self, region: Region, depth: usize) {
        self.coarsest_homogeneous_depth[region.level][Self::at(region)] = depth as u8;
    }

    pub fn cost_of(&self, region: Region) -> usize {
        self.cheapest_description[region.level][Self::at(region)]
    }

    pub fn set_cost(&mut self, region: Region, cost: usize) {
        self.cheapest_description[region.level][Self::at(region)] = cost;
    }

    /// Takes a region's cells back off the encoded map, so that a
    /// pass over its children can be made again from nothing.
    pub fn unmark_region(&mut self, region: Region) {
        let (x, y) = region.top_left_cell();
        let side = region.side_in_cells();
        self.encoded_cells.unset_rect(
            x as i64,
            y as i64,
            (x + side - 1) as i64,
            (y + side - 1) as i64,
        );
    }

    /// Marks every cell of a region encoded.
    pub fn mark_encoded(&mut self, region: Region) {
        let (x, y) = region.top_left_cell();
        let side = region.side_in_cells();
        self.encoded_cells.set_rect(
            x as i64,
            y as i64,
            (x + side - 1) as i64,
            (y + side - 1) as i64,
        );
    }
}
