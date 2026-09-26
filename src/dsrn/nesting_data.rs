//! What an encode says, what it produces, and the room it works in.
//!
//! Nothing here does anything. What to say about a region is
//! [`super::coarsest`] and [`super::encode`]; how to say it is
//! [`super::encode`]; how much it costs is [`super::cost`]. This is
//! the vocabulary all three share.

use crate::dsrn::region::{Region, CHILD_COUNT, EVERY_CHILD};
use crate::dsrn::stream::EncodedBitmap;
use crate::dsrn::region::deepest_depth;
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

/// The tile size a 4x4 has no size for, which is the one after its
/// coarsest: it can be bound at 4x4, 2x2 or 1x1 tiles, which is three
/// of the four a two bit field can name.
///
/// The fourth says something else entirely: that the four children
/// copy, each from its own neighbour. A 2x2 has no grammar of its own
/// and so can never say copy, and this is the only place it can be
/// said for it. It costs nothing to have, because the field it is
/// written in was being paid for anyway.
pub const A_SIZE_THAT_MEANS_COPY: u64 = (CELL_LEVEL - FINEST_LEVEL_WITH_A_GRAMMAR + 1) as u64;

/// How many tile sizes a region at a level can be covered with, from
/// being one tile itself down to being covered a cell at a time.
const fn tile_sizes_at(level: usize) -> usize {
    deepest_depth(level) + 1
}

/// What a region can already count on before it says anything: what
/// the closest binding above it says over its cells.
///
/// A region left out of its parent's mask is not left clear, it is
/// left to that binding. So what a region may leave out depends on
/// what the binding above would put there, and that is this.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Standing {
    /// No binding above at all. What is left unsaid here stays as the
    /// decoder found it, which is clear -- so this is `Reads(false)`
    /// for everything but who puts it there, which is nobody.
    Clear,
    /// A binding above covers this region and says this over it.
    /// Everything left unsaid here reads it.
    Reads(bool),
    /// A binding above covers this region with tiles `depth` levels
    /// below it. At zero the region is one of those tiles, and the
    /// bit that says it is the region's to keep or to take. Deeper
    /// than that there is no one value to leave anything to -- each
    /// tile has its own -- but there is still something to leave
    /// things to, which is those tiles.
    Tiles(usize),
    /// Nothing to leave anything to. A copy covers this region, and
    /// what it puts here reads the neighbour rather than any one
    /// thing.
    Nothing,
}

impl Standing {
    /// What is already there to be left alone, where there is any one
    /// thing.
    pub fn reads(self) -> Option<bool> {
        match self {
            Standing::Clear => Some(false),
            Standing::Reads(value) => Some(value),
            Standing::Tiles(_) | Standing::Nothing => None,
        }
    }
}

/// Which children of a region get a description of their own, and
/// which are left to the closest binding above them.
///
/// The mask does not change what its code does. A binding still
/// covers every cell of its region, a copy still takes the whole of
/// one. What the mask says is which children are described again,
/// over the top -- and a child described that way is an override: it
/// writes its own cells, and the binding above writes no bit for a
/// tile that falls inside it.
///
/// A child the mask does not name is not left clear. It is left to
/// the closest binding above it, which is what covers it. Only where
/// there is no such binding does a child left out stay as the decoder
/// found it.
///
/// A bit set names a child that is described. None set is the
/// unmasked case for a binding and a copy, which describe no child
/// again; all four set is the unmasked case for subdividing, which
/// says nothing itself and so has to describe all four.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RegionMask(pub u64);

impl RegionMask {
    pub const EVERY: Self = Self(EVERY_CHILD);
    pub const NONE: Self = Self(0);

    /// Whether the mask names a child as described.
    pub fn describes(self, child: usize) -> bool {
        self.0 >> child & 1 == 1
    }

    /// How many children it describes.
    pub fn described(self) -> usize {
        self.0.count_ones() as usize
    }

    /// How many it leaves to the binding above.
    pub fn left_to_a_binding(self) -> usize {
        CHILD_COUNT - self.described()
    }
}

/// What a region says about itself, and the mask that goes with it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RegionCode {
    /// Bound at a tile size `depth` levels below the region: a bit
    /// for every tile of it that no description below has taken.
    Bind { level: usize, depth: usize, mask: RegionMask },
    /// Nothing said. The children the mask names describe
    /// themselves; the rest are left to the closest binding above.
    Subdivide { mask: RegionMask },
    /// Taken from the neighbour in a direction, but for the children
    /// the mask names.
    Copy { direction: usize, mask: RegionMask },
    /// Only a 4x4 says this. The children the mask names each copy
    /// from a neighbour of their own; the rest are left to the
    /// closest binding above, which already says what they hold.
    CopyEachChild { mask: RegionMask },
}

impl RegionCode {
    pub fn mask(self) -> RegionMask {
        match self {
            RegionCode::Bind { mask, .. }
            | RegionCode::Subdivide { mask }
            | RegionCode::Copy { mask, .. }
            | RegionCode::CopyEachChild { mask } => mask,
        }
    }

    /// Whether it needs the mark and the mask written out.
    ///
    /// Subdividing is masked whenever it describes fewer than four,
    /// because subdividing without a mask describes all four; the
    /// other two are masked whenever they describe any child at all,
    /// because unmasked they describe none.
    pub fn is_masked(self) -> bool {
        match self {
            RegionCode::Subdivide { mask } => mask != RegionMask::EVERY,
            // Its mask is not a modifier on something else. It is the
            // whole of what it says, and it is always written.
            RegionCode::CopyEachChild { .. } => false,
            _ => self.mask() != RegionMask::NONE,
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
    /// children a mask left to the closest binding above them.
    pub children_made_regions: usize,
    pub children_left_to_a_binding: usize,
    /// Regions below the grammar, which wrote their cells and no code.
    pub below_the_grammar: usize,
    /// 4x4s that masked their four children by definition, and how
    /// many of those children were copied rather than written out.
    pub four_by_four_masks: usize,
    pub children_copied: usize,
    /// 4x4s whose children each copied from a neighbour of their own,
    /// and how many of those children there were.
    pub four_by_fours_copying_each_child: usize,
    pub children_copying_themselves: usize,
    /// 4x4s that said it in a grammar of their own.
    pub four_by_fours_in_their_own_grammar: usize,
    /// Of those, the ones every cell of which was already right --
    /// the ones that had nothing to say and had to say something --
    /// and the ones taken whole from a neighbour.
    pub four_by_fours_already_right: usize,
    pub four_by_fours_copied_whole: usize,
    /// And the ones that said nothing at all, leaving the whole of
    /// themselves to the binding above.
    pub four_by_fours_left_whole: usize,
    /// What the already right ones spent saying what they had no way
    /// of not saying.
    pub bits_spent_on_being_already_right: usize,
    /// What each of the four codes was said, and what each spent.
    pub four_by_four_said: [usize; 4],
    pub four_by_four_spent: [usize; 4],
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
    /// Per region, what its cheapest description costs when it has
    /// to say the whole of itself.
    pub cheapest_description: Vec<Vec<usize>>,
    /// Per region, what its cheapest description costs when a binding
    /// above already says one thing over it -- once for each thing
    /// that binding could be saying.
    ///
    /// Zero where the region already reads that: there is nothing to
    /// put right, and the region is not described at all. Which of
    /// the two a binding should say is the one choice a tile has that
    /// nothing else decides, and it is one bit, so both are priced
    /// and the cheaper wins.
    pub cost_to_put_right: Vec<Vec<[usize; 2]>>,
    /// Per region, and per tile size a binding above could be
    /// covering it with, what it costs to describe it -- counting the
    /// binding's own payload bits for the tiles inside it that
    /// survive being described.
    ///
    /// Those bits belong here and not to the binding because whether
    /// they survive is decided here. A region that takes the whole of
    /// itself leaves the binding nothing to say about its area and
    /// none of them are written; a region that puts right a corner of
    /// itself leaves all the rest, and every one of them is.
    ///
    /// Indexed by the tile size as a depth below the region, from
    /// zero -- the region is itself one tile -- down to its cells.
    pub cost_under_tiles: Vec<Vec<usize>>,
    /// Which cells some description has taken, and so which cells
    /// the decoder will already hold.
    ///
    /// It answers both questions the descent asks about ground it has
    /// been over: whether a region may be copied from, and whether a
    /// tile of a binding falls inside something below that took it
    /// first. Every cell is taken exactly once -- that is what makes
    /// the regions disjoint -- so the binding above writes bits only
    /// for what is left.
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
            cost_to_put_right: (0..=CELL_LEVEL)
                .map(|level| vec![[0usize; 2]; tiles_in_level(level)])
                .collect(),
            cost_under_tiles: (0..=CELL_LEVEL)
                .map(|level| vec![0usize; tiles_in_level(level) * tile_sizes_at(level)])
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

    /// What it costs to put a region right when the binding above it
    /// says `standing` over it.
    pub fn cost_to_put_right_of(&self, region: Region, standing: bool) -> usize {
        self.cost_to_put_right[region.level][Self::at(region)][standing as usize]
    }

    pub fn set_cost_to_put_right(&mut self, region: Region, standing: bool, cost: usize) {
        self.cost_to_put_right[region.level][Self::at(region)][standing as usize] = cost;
    }

    /// Which of the two things a binding could say over a region
    /// leaves the least to put right.
    pub fn value_worth_standing(&self, region: Region) -> bool {
        self.cost_to_put_right_of(region, true) < self.cost_to_put_right_of(region, false)
    }

    /// What it costs to describe a region a binding above covers with
    /// tiles `depth` levels below it, the binding's surviving payload
    /// bits for that area included.
    pub fn cost_under_tiles_of(&self, region: Region, depth: usize) -> usize {
        self.cost_under_tiles[region.level][Self::under(region, depth)]
    }

    pub fn set_cost_under_tiles(&mut self, region: Region, depth: usize, cost: usize) {
        self.cost_under_tiles[region.level][Self::under(region, depth)] = cost;
    }

    fn under(region: Region, depth: usize) -> usize {
        Self::at(region) * tile_sizes_at(region.level) + depth
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
