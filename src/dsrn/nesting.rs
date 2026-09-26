//! DSRN written plainly: disjoint sized-tile region nesting.
//!
//! Nothing here is written for speed. Every question is asked the way
//! the algorithm states it, a cell at a time where that is what the
//! statement says, so that what the encoding costs can be read off
//! the code rather than argued about. The pyramid stays, because it
//! is part of the algorithm rather than a trick: it answers "is this
//! square all one thing" and that question is the whole of DSRN.
//!
//! # Regions, tiles, children
//!
//! A **region** is a node of the quadtree. Its four **children** are
//! its quadrants. A **tile** is an aligned square of an aligned size,
//! and a region is filled by tiles of whatever size it names. Tiles
//! and regions line up, which is what the encoding lives off: a tile
//! is always some region, so a tile that needs describing separately
//! can describe itself.
//!
//! # The codes
//!
//! Every region writes two bits. The decoder always knows which
//! region it stands on, so nothing says where it is or how big.
//!
//! ```text
//! 00  bind        a tile size, then a payload bit for every tile
//! 01  subdivide   the four children, in reading order
//! 10  copy        a direction
//! 11  mask        one of the three above, then four bits saying which
//!                 children it leaves alone and which become regions
//! ```
//!
//! The fourth is not a fourth thing to say. It is the other three
//! said of part of a region: the mask names the children the
//! operation leaves alone, and the rest follow as regions of their
//! own. So a masked binding fills the children it leaves alone and
//! hands the others down, a masked copy takes the children it leaves
//! alone from a neighbour, and a masked subdivide leaves its children
//! alone entirely -- which is to say clear, since that is what the
//! decoder starts from.
//!
//! Nothing costs a bit for the option. A region that masks pays two
//! bits for the mark and four for the mask; a region that does not
//! pays neither. That is the whole reason the mark is a code rather
//! than a flag on a binding: a flag is paid by every binding whether
//! it masks or not.
//!
//! `11` is never followed by `11`. A mask modifies one of the three,
//! and masking a mask would say nothing the one mask cannot.
//!
//! # Tile size
//!
//! A binding names its tile's side as a power of two, in as many bits
//! as its own size allows -- none at a cell, four at the whole
//! bitmap. See [`size_width`].
//!
//! An unmasked binding covers its whole region and writes one bit per
//! tile, so every tile of it has to be homogeneous. Coarser than the
//! coarsest size that manages that and a payload bit would be a lie;
//! finer and the same answer goes out four times over. A masked
//! binding only has to be true of the children it keeps, which is
//! what lets it name a coarser size than the region as a whole could
//! carry and hand the awkward children down.
//!
//! # Copying
//!
//! A region may copy from the four neighbours of its own size that
//! reading order puts before it. Two things have to hold: the cells
//! have to match, and the decoder has to hold them already. The
//! second is not a property of the bitmap -- it depends on what every
//! region above chose -- so the descent keeps a plain map of which
//! cells it has described and asks it outright.

use crate::dsrn::region::{children_of, Region, DIRECTIONS};
use crate::dsrn::stream::Bits;
use crate::dsrn::{Pyramid, LEVELS};
use crate::BitMatrix;

/// The smallest region allowed to mask.
///
/// Masking is only ever paid for where it is used, so forbidding it
/// cannot make an encoding smaller by itself -- it can only take an
/// option away from a region that would have chosen it. What it can
/// change is everything above: a region's cost is what its children
/// cost, so a rule that makes small regions dearer makes their
/// parents choose differently.
///
/// Nothing in the stream says which of these was used. A mask is a
/// code the decoder reads when it finds it, so this is a rule the
/// encoder keeps to and the decoder never needs to know.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Masking {
    /// Any region with children, which is anything above a cell.
    Anywhere,
    From4,
    From8,
    From16,
}

impl Masking {
    pub const ALL: [Masking; 4] =
        [Masking::Anywhere, Masking::From4, Masking::From8, Masking::From16];

    /// The side of the smallest region that may mask.
    pub fn smallest(self) -> usize {
        1 << self.level()
    }

    /// That region's level.
    fn level(self) -> usize {
        match self {
            Masking::Anywhere => 1,
            Masking::From4 => 2,
            Masking::From8 => 3,
            Masking::From16 => 4,
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
    fn allows(self, region: Region) -> bool {
        region.level >= self.level()
    }
}

/// The two bit code every region writes.
const BIND: u64 = 0b00;
const SUBDIVIDE: u64 = 0b01;
const COPY: u64 = 0b10;
/// Not a fourth thing to say: a mark that one of the three above is
/// about to be said of part of the region.
const MASK: u64 = 0b11;

/// The widths, in bits.
const CODE: usize = 2;

/// The width of a binding's tile size field for a region at `level`.
///
/// A region of side `1 << level` can name `level + 1` tile sizes, the
/// cells up to the region itself, so it spends exactly the bits those
/// need: none at a cell, one at a 2x2, four at the whole bitmap. A
/// flat field wide enough for the whole bitmap would charge a 2x2
/// four bits to say "cells", and over a corpus that is 1652 bits a
/// bitmap -- more than the mask code earns.
fn size_width(level: usize) -> usize {
    let mut width = 0;
    while (1 << width) < level + 1 {
        width += 1;
    }
    width
}
const DIRECTION: usize = 2;
const CHILD_MASK: usize = 4;

/// A child mask naming every child.
const EVERY_CHILD: u64 = 0b1111;

/// Tiles in a region whose tiles are `depth` levels below it.
fn tiles(depth: usize) -> usize {
    1 << (2 * depth)
}

/// What a binding of a region at `level` spends before its payload.
pub fn head_of_a_binding(level: usize) -> usize {
    CODE + size_width(level)
}

/// How many of each code an encode wrote.
#[derive(Default, Clone, Copy)]
pub struct Counts {
    pub bindings: usize,
    pub subdivides: usize,
    pub copies: usize,
    /// Of each of those, the ones that masked.
    pub masked_bindings: usize,
    pub masked_subdivides: usize,
    pub masked_copies: usize,
    /// Children a mask sent off to be regions of their own.
    pub children_made_regions: usize,
    /// Children a masked subdivide left clear, which cost nothing at
    /// all beyond their bit of the mask.
    pub children_left_clear: usize,
    /// Regions bound at one cell a tile, and the payload bits that
    /// went out one cell at a time -- the encoding at its floor.
    pub bound_at_cells: usize,
    pub cells_written: usize,
    /// The cells those regions wrote, by the level of the region that
    /// gave up.
    pub cells_given_up: [usize; LEVELS + 1],
    /// Of those regions, the ones holding the same cells as a
    /// neighbour they may copy from -- but one the decoder will not
    /// hold by the time it arrives.
    pub copies_just_missed: usize,
    /// And the ones with no matching neighbour at all.
    pub no_neighbour_matched: usize,
    /// Every bit this encode believes it wrote, added up one region
    /// at a time from the mask arithmetic rather than from the loops
    /// that write them. If it is not [`Encoded::bits`], one of the
    /// two is wrong.
    pub accounted: usize,
}

/// What an encode produces.
#[derive(Default)]
pub struct Encoded {
    pub counts: Counts,
    /// The codes, tile sizes, child masks and directions.
    pub tree: Bits,
    /// One bit per tile, in the order the regions bound them.
    pub payload: Bits,
}

impl Encoded {
    /// Every bit of the encoding.
    pub fn bits(&self) -> usize {
        self.tree.len() + self.payload.len()
    }

    fn clear(&mut self) {
        self.counts = Counts::default();
        self.tree.clear();
        self.payload.clear();
    }
}

/// What a region settled on. `alone` is the mask: the children the
/// operation covers, with the rest becoming regions of their own. All
/// four means no mask at all, and no mark to introduce one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Says {
    Bind { level: usize, depth: usize, alone: u64 },
    Subdivide { alone: u64 },
    Copy { dir: usize, alone: u64 },
}

impl Says {
    /// The children the description covers. The rest become regions.
    fn alone(self) -> u64 {
        match self {
            Says::Bind { alone, .. } | Says::Subdivide { alone } | Says::Copy { alone, .. } => alone,
        }
    }

    fn masked(self) -> bool {
        match self {
            // Subdividing is masked whenever it leaves any child
            // alone, because subdividing without a mask leaves none.
            Says::Subdivide { alone } => alone != 0,
            Says::Bind { alone, .. } | Says::Copy { alone, .. } => alone != EVERY_CHILD,
        }
    }
}

/// The room an encode works in. One entry per region of every level,
/// found once and refilled per bitmap.
pub struct Workspace {
    /// The coarsest tile size at which every tile of a region is
    /// homogeneous, as a depth below the region.
    finest: Vec<Vec<u8>>,
    /// What the cheapest description of a region costs.
    cost: Vec<Vec<usize>>,
    /// Which cells the descent has described so far, which is what
    /// the decoder will hold when it arrives.
    written: BitMatrix,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            finest: (0..=LEVELS)
                .map(|level| vec![0u8; Pyramid::side(level) * Pyramid::side(level)])
                .collect(),
            cost: (0..=LEVELS)
                .map(|level| vec![0usize; Pyramid::side(level) * Pyramid::side(level)])
                .collect(),
            written: BitMatrix::new(),
        }
    }

    fn at(region: Region) -> usize {
        region.y * Pyramid::side(region.level) + region.x
    }

    fn finest_of(&self, region: Region) -> usize {
        self.finest[region.level][Self::at(region)] as usize
    }

    fn cost_of(&self, region: Region) -> usize {
        self.cost[region.level][Self::at(region)]
    }
}

/// The cells of a region, as a rectangle.
fn cells(region: Region) -> (usize, usize, usize) {
    let side = 1usize << region.level;
    (region.x * side, region.y * side, side)
}

/// Whether two regions of the same size hold the same cells. A cell
/// at a time, as the statement says.
fn same_cells(bits: &BitMatrix, a: Region, b: Region) -> bool {
    let ((ax, ay, side), (bx, by, _)) = (cells(a), cells(b));
    for row in 0..side {
        for col in 0..side {
            if bits.get((ax + col) as u8, (ay + row) as u8)
                != bits.get((bx + col) as u8, (by + row) as u8)
            {
                return false;
            }
        }
    }
    true
}

/// The neighbour of a region in a direction, if it is on the bitmap.
fn neighbour(region: Region, dir: usize) -> Option<Region> {
    let (dx, dy) = DIRECTIONS[dir];
    let (x, y) = (region.x as isize + dx, region.y as isize + dy);
    let across = Pyramid::side(region.level) as isize;
    (x >= 0 && y >= 0 && x < across && y < across).then(|| Region {
        level: region.level,
        x: x as usize,
        y: y as usize,
    })
}

/// Whether the decoder will already hold every cell of a region.
fn already_written(work: &Workspace, region: Region) -> bool {
    let (x, y, side) = cells(region);
    for row in 0..side {
        for col in 0..side {
            if !work.written.get((x + col) as u8, (y + row) as u8) {
                return false;
            }
        }
    }
    true
}

/// Marks every cell of a region described.
fn mark_written(work: &mut Workspace, region: Region) {
    let (x, y, side) = cells(region);
    work.written.set_rect(x as i64, y as i64, (x + side - 1) as i64, (y + side - 1) as i64);
}

/// What a region holds, if it is all one thing.
fn homogeneous(pyramid: &Pyramid, bits: &BitMatrix, region: Region) -> Option<bool> {
    if region.level == 0 {
        // The pyramid starts at the 2x2 squares; the bitmap is the
        // cells.
        return Some(bits.get(region.x as u8, region.y as u8));
    }
    pyramid.at(region.level, region.x, region.y)
}

/// Whether every cell of a region is clear, which is what a masked
/// subdivide leaves its children as.
fn all_clear(pyramid: &Pyramid, bits: &BitMatrix, region: Region) -> bool {
    homogeneous(pyramid, bits, region) == Some(false)
}

/// The tiles of a region at a tile size, in reading order.
fn tiles_of(region: Region, depth: usize) -> Vec<Region> {
    let across = 1usize << depth;
    let level = region.level - depth;
    let mut out = Vec::with_capacity(tiles(depth));
    for row in 0..across {
        for col in 0..across {
            out.push(Region { level, x: region.x * across + col, y: region.y * across + row });
        }
    }
    out
}

/// Which child of a region a tile falls in, at that tile size.
fn child_of_tile(region: Region, depth: usize, tile: Region) -> usize {
    let half = 1usize << (depth - 1);
    let (col, row) = (tile.x - region.x * (1 << depth), tile.y - region.y * (1 << depth));
    (row >= half) as usize * 2 + (col >= half) as usize
}

/// What a description spends on itself: everything but the regions
/// it hands on.
///
/// The mark and the mask are paid only when there is a mask -- when
/// the operation leaves every child alone (or, for subdividing, none)
/// there is nothing to say and nothing to pay.
///
/// The payload term is the whole of the answer to "does a binding
/// emit bits for tiles inside a region it handed on": it does not.
/// A masked binding pays one bit per tile of the children it keeps,
/// and the children it hands on are not counted here or anywhere --
/// they pay for themselves.
fn bits_of_its_own(says: Says) -> usize {
    let mut cost = CODE + if says.masked() { CODE + CHILD_MASK } else { 0 };
    match says {
        Says::Bind { depth, alone, .. } => {
            cost += size_width_of(says);
            cost += if alone == EVERY_CHILD {
                tiles(depth)
            } else {
                alone.count_ones() as usize * tiles(depth - 1)
            };
        }
        Says::Subdivide { .. } => {}
        Says::Copy { .. } => cost += DIRECTION,
    }
    cost
}

/// A binding carries its region's level so that its tile size field
/// can be measured without the region to hand.
fn size_width_of(says: Says) -> usize {
    match says {
        Says::Bind { level, .. } => size_width(level),
        _ => 0,
    }
}

/// What a description costs all told: what it spends on itself, and
/// what the regions it hands on will spend.
fn cost_of_saying(work: &Workspace, region: Region, says: Says) -> usize {
    let mut cost = bits_of_its_own(says);
    let alone = says.alone();
    for (bit, child) in children_of(region).into_iter().enumerate() {
        if alone >> bit & 1 == 0 {
            cost += work.cost_of(child);
        }
    }
    cost
}

/// Which children a binding at a tile size can keep, and which it has
/// to hand down.
///
/// A child whose tiles are not all homogeneous at this size has to go
/// down, or its share of the payload would be a lie. A child cheaper
/// to describe than its share wants to go down.
fn kept_by_a_binding(work: &Workspace, region: Region, depth: usize) -> u64 {
    let share = tiles(depth - 1);
    let mut alone = 0;
    for (bit, child) in children_of(region).into_iter().enumerate() {
        if work.finest_of(child) <= depth - 1 && work.cost_of(child) >= share {
            alone |= 1 << bit;
        }
    }
    alone
}

/// Every description a region could give of itself, cheapest last so
/// that a fold over them takes the best.
fn every_way(
    work: &Workspace,
    pyramid: &Pyramid,
    bits: &BitMatrix,
    region: Region,
    masking: Masking,
    written: bool,
) -> Vec<Says> {
    let mut ways = Vec::new();
    let may_mask = masking.allows(region);

    for depth in 0..=region.level {
        // Unmasked: every tile of the region has to be homogeneous.
        if depth >= work.finest_of(region) {
            ways.push(Says::Bind { level: region.level, depth, alone: EVERY_CHILD });
        }
        // Masked: only the children it keeps have to be, and a mask
        // names children, so there have to be children to name.
        if depth >= 1 && may_mask {
            let alone = kept_by_a_binding(work, region, depth);
            if alone != EVERY_CHILD {
                ways.push(Says::Bind { level: region.level, depth, alone });
            }
        }
    }

    if region.level > 0 {
        ways.push(Says::Subdivide { alone: 0 });
        // Masked: the children it leaves alone stay clear.
        let mut clear = 0;
        for (bit, child) in children_of(region).into_iter().enumerate() {
            if all_clear(pyramid, bits, child) {
                clear |= 1 << bit;
            }
        }
        if may_mask && clear != 0 && clear != EVERY_CHILD {
            ways.push(Says::Subdivide { alone: clear });
        }

        for dir in 0..DIRECTIONS.len() {
            let Some(from) = neighbour(region, dir) else { continue };
            let here = |a: Region, b: Region| {
                same_cells(bits, a, b) && (!written || already_written(work, b))
            };
            if here(region, from) {
                ways.push(Says::Copy { dir, alone: EVERY_CHILD });
                continue;
            }
            let mut alone = 0;
            for (bit, (child, mirror)) in
                children_of(region).into_iter().zip(children_of(from)).enumerate()
            {
                if here(child, mirror) {
                    alone |= 1 << bit;
                }
            }
            if may_mask && alone != 0 {
                ways.push(Says::Copy { dir, alone });
            }
        }
    }

    ways
}

/// Reads the pyramid bottom up, leaving every region the coarsest
/// tile size that tiles it homogeneously and what its cheapest
/// description costs.
///
/// The cost of a copy is what it would cost if the neighbour were
/// already written. Whether it will be depends on what every region
/// above this one chooses, which happens later, so this cannot know
/// -- and it does not have to. Pricing a copy at four bits only ever
/// makes a region look cheaper than it turns out to be, and the
/// descent asks the real question before it writes anything.
fn survey(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bits: &BitMatrix,
    region: Region,
    masking: Masking,
) {
    let at = Workspace::at(region);

    if region.level == 0 {
        work.finest[0][at] = 0;
        work.cost[0][at] = CODE + size_width(0) + 1;
        return;
    }

    let plain = homogeneous(pyramid, bits, region).is_some();
    let mut deepest = 0;
    for child in children_of(region) {
        survey(work, pyramid, bits, child, masking);
        deepest = deepest.max(work.finest_of(child));
    }
    work.finest[region.level][at] = if plain { 0 } else { (deepest + 1) as u8 };

    let mut best = usize::MAX;
    for says in every_way(work, pyramid, bits, region, masking, false) {
        best = best.min(cost_of_saying(work, region, says));
    }
    work.cost[region.level][at] = best;
}

/// Encodes the bitmap. The pyramid must already hold it.
pub fn encode(
    pyramid: &Pyramid,
    bits: &BitMatrix,
    masking: Masking,
    work: &mut Workspace,
    out: &mut Encoded,
) {
    out.clear();
    work.written.words.fill(0);
    let whole = Region { level: LEVELS, x: 0, y: 0 };
    survey(work, pyramid, bits, whole, masking);
    describe(work, pyramid, bits, whole, masking, out);
}

/// Describes one region, and whatever its description leaves out.
fn describe(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bits: &BitMatrix,
    region: Region,
    masking: Masking,
    out: &mut Encoded,
) {
    let says = every_way(work, pyramid, bits, region, masking, true)
        .into_iter()
        .min_by_key(|&says| cost_of_saying(work, region, says))
        .expect("every region can at least bind at one cell a tile");

    out.counts.accounted += bits_of_its_own(says);
    if let Says::Bind { depth, .. } = says {
        out.counts.bindings += 1;
        if region.level == depth && region.level > 0 {
            out.counts.bound_at_cells += 1;
            out.counts.cells_given_up[region.level] += tiles(depth);
            // Whether a copy was there to be had and reading order
            // took it away, or there was never one.
            let matched = (0..DIRECTIONS.len())
                .any(|dir| neighbour(region, dir).is_some_and(|f| same_cells(bits, region, f)));
            if matched {
                out.counts.copies_just_missed += 1;
            } else {
                out.counts.no_neighbour_matched += 1;
            }
        }
    }

    let alone = says.alone();
    if says.masked() {
        out.tree.push(MASK, CODE);
    }
    match says {
        Says::Bind { depth, .. } => {
            out.tree.push(BIND, CODE);
            if says.masked() {
                out.counts.masked_bindings += 1;
                out.tree.push(alone, CHILD_MASK);
            }
            out.tree.push((region.level - depth) as u64, size_width(region.level));
            // A payload bit for every tile of every child it keeps,
            // in reading order.
            for tile in tiles_of(region, depth) {
                if alone != EVERY_CHILD && alone >> child_of_tile(region, depth, tile) & 1 == 0 {
                    continue;
                }
                let value = homogeneous(pyramid, bits, tile)
                    .expect("a bound tile is homogeneous, or the binding would be a lie");
                out.payload.push(value as u64, 1);
                if tile.level == 0 {
                    out.counts.cells_written += 1;
                }
                mark_written(work, tile);
            }
        }
        Says::Subdivide { .. } => {
            out.counts.subdivides += 1;
            out.tree.push(SUBDIVIDE, CODE);
            if says.masked() {
                out.counts.masked_subdivides += 1;
                out.counts.children_left_clear += alone.count_ones() as usize;
                out.tree.push(alone, CHILD_MASK);
            }
            // A child left alone is left clear, which the decoder
            // already holds it as.
            for (bit, child) in children_of(region).into_iter().enumerate() {
                if alone >> bit & 1 == 1 {
                    mark_written(work, child);
                }
            }
        }
        Says::Copy { dir, .. } => {
            out.counts.copies += 1;
            out.tree.push(COPY, CODE);
            if says.masked() {
                out.counts.masked_copies += 1;
                out.tree.push(alone, CHILD_MASK);
            }
            out.tree.push(dir as u64, DIRECTION);
            for (bit, child) in children_of(region).into_iter().enumerate() {
                if alone >> bit & 1 == 1 {
                    mark_written(work, child);
                }
            }
            if alone == EVERY_CHILD {
                mark_written(work, region);
            }
        }
    }

    // Whatever the description left out is described before the
    // descent moves on, so a region beside it may read what it wrote.
    for (bit, child) in children_of(region).into_iter().enumerate() {
        if region.level > 0 && alone >> bit & 1 == 0 {
            out.counts.children_made_regions += 1;
            describe(work, pyramid, bits, child, masking, out);
        }
    }
}

/// The decoder's place in the two streams.
#[derive(Default)]
struct Reading {
    tree: usize,
    payload: usize,
}

impl Reading {
    fn take(&mut self, out: &Encoded, width: usize) -> u64 {
        let got = out.tree.take(self.tree, width).unwrap_or(0);
        self.tree += width;
        got
    }

    fn value(&mut self, out: &Encoded) -> bool {
        let got = out.payload.take(self.payload, 1).unwrap_or(0) == 1;
        self.payload += 1;
        got
    }
}

/// Reads the bitmap back.
pub fn decode(out: &Encoded, bits: &mut BitMatrix) {
    bits.words.fill(0);
    let mut reading = Reading::default();
    undescribe(&mut reading, out, bits, Region { level: LEVELS, x: 0, y: 0 });
}

/// Writes a whole tile into the bitmap.
fn fill(bits: &mut BitMatrix, tile: Region, value: bool) {
    if !value {
        return;
    }
    let (x, y, side) = cells(tile);
    bits.set_rect(x as i64, y as i64, (x + side - 1) as i64, (y + side - 1) as i64);
}

/// Copies one region's cells onto another's, a cell at a time.
fn copy_cells(bits: &mut BitMatrix, to: Region, from: Region) {
    let ((tx, ty, side), (fx, fy, _)) = (cells(to), cells(from));
    for row in 0..side {
        for col in 0..side {
            if bits.get((fx + col) as u8, (fy + row) as u8) {
                bits.set((tx + col) as u8, (ty + row) as u8);
            }
        }
    }
}

/// Puts back one region, and whatever its description left out.
fn undescribe(reading: &mut Reading, out: &Encoded, bits: &mut BitMatrix, region: Region) {
    let mut code = reading.take(out, CODE);
    let masked = code == MASK;
    if masked {
        code = reading.take(out, CODE);
    }
    // Unmasked, a binding and a copy cover the whole region and a
    // subdivision covers none of it.
    let alone = if masked {
        reading.take(out, CHILD_MASK)
    } else if code == SUBDIVIDE {
        0
    } else {
        EVERY_CHILD
    };

    match code {
        BIND => {
            let depth = region.level - reading.take(out, size_width(region.level)) as usize;
            for tile in tiles_of(region, depth) {
                if alone != EVERY_CHILD && alone >> child_of_tile(region, depth, tile) & 1 == 0 {
                    continue;
                }
                let value = reading.value(out);
                fill(bits, tile, value);
            }
        }
        COPY => {
            let dir = reading.take(out, DIRECTION) as usize;
            let from = neighbour(region, dir).expect("a copy names a neighbour on the bitmap");
            if alone == EVERY_CHILD {
                copy_cells(bits, region, from);
            } else {
                for (bit, (child, mirror)) in
                    children_of(region).into_iter().zip(children_of(from)).enumerate()
                {
                    if alone >> bit & 1 == 1 {
                        copy_cells(bits, child, mirror);
                    }
                }
            }
        }
        // Subdividing says nothing, and a child it leaves alone stays
        // as the decoder found it, which is clear.
        _ => {}
    }

    for (bit, child) in children_of(region).into_iter().enumerate() {
        if region.level > 0 && alone >> bit & 1 == 0 {
            undescribe(reading, out, bits, child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples;

    fn checkerboard(side: usize) -> BitMatrix {
        let mut bits = BitMatrix::new();
        for y in 0..256 {
            for x in 0..256 {
                if (x / side + y / side) % 2 == 0 {
                    bits.set(x as u8, y as u8);
                }
            }
        }
        bits
    }

    fn cases() -> Vec<BitMatrix> {
        let mut cases = vec![BitMatrix::new()];
        for shape in samples::SHAPES {
            cases.extend(shape.tested());
        }
        cases.extend(samples::grown(0, 1.0, 0.0, 1));
        // Laid out on the quadtree's own grid, which is the shape
        // this is for and the one the grown samples never give.
        for plan in &samples::PLANS {
            cases.extend(plan.tested());
        }
        for side in [1usize, 2, 3, 8, 16] {
            cases.push(checkerboard(side));
        }
        let mut one = BitMatrix::new();
        one.set(128, 128);
        cases.push(one);
        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        cases.push(full);
        let mut stripes = BitMatrix::new();
        for y in 0..256 {
            if y % 3 == 0 {
                stripes.set_rect(0, y, 255, y);
            }
        }
        cases.push(stripes);
        // A 16x16 block with an aligned 2x2 hole, which is what a
        // masked binding is for.
        let mut hole = BitMatrix::new();
        hole.set_rect(64, 64, 79, 79);
        hole.unset_rect(74, 74, 75, 75);
        cases.push(hole);
        cases
    }

    /// The encoding comes back the bitmap that went in. Nothing else
    /// about it matters if this is ever false.
    #[test]
    fn the_encoding_comes_back_the_bitmap_that_went_in() {
        let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
        let (mut out, mut back) = (Encoded::default(), BitMatrix::new());
        for masking in Masking::ALL {
            for (case, bits) in cases().iter().enumerate() {
                pyramid.clear();
                pyramid.rebuild(bits);
                encode(&pyramid, bits, masking, &mut work, &mut out);
                decode(&out, &mut back);
                for y in 0..=u8::MAX {
                    for x in 0..=u8::MAX {
                        assert_eq!(
                            bits.get(x, y),
                            back.get(x, y),
                            "masking {}, case {case}, differs at ({x}, {y})",
                            masking.name()
                        );
                    }
                }
            }
        }
    }

    /// A bitmap of one value is one binding. Three bits of tile size
    /// cannot name a tile of 256, so it binds at 128 and spends four
    /// payload bits where one would have done.
    #[test]
    fn a_bitmap_of_one_value_is_one_binding() {
        let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
        let mut out = Encoded::default();
        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        for bits in [BitMatrix::new(), full] {
            pyramid.clear();
            pyramid.rebuild(&bits);
            encode(&pyramid, &bits, Masking::Anywhere, &mut work, &mut out);
            assert_eq!(out.counts.bindings, 1);
            assert_eq!(out.counts.masked_bindings, 0);
            assert_eq!(out.bits(), CODE + size_width(LEVELS) + 1);
        }
    }

    /// Nothing pays for a mask it does not use: an unmasked
    /// description writes no mark and no mask.
    #[test]
    fn an_unmasked_description_pays_for_no_mask() {
        let whole = Says::Bind { level: 4, depth: 2, alone: EVERY_CHILD };
        assert!(!whole.masked());
        assert_eq!(bits_of_its_own(whole), CODE + size_width(4) + tiles(2));

        // Masked, it pays the mark, the mask, and a payload bit for
        // every tile of the three children it kept -- and nothing at
        // all for the one it handed on.
        let handing_one_on = Says::Bind { level: 4, depth: 2, alone: 0b0111 };
        assert!(handing_one_on.masked());
        assert_eq!(
            bits_of_its_own(handing_one_on),
            CODE + CODE + CHILD_MASK + size_width(4) + 3 * tiles(1)
        );
        assert!(!Says::Subdivide { alone: 0 }.masked());
        assert!(Says::Subdivide { alone: 0b0001 }.masked());
    }

    /// Every bit written is one region's own, and a binding writes
    /// none for a tile inside a region it handed on.
    ///
    /// The count is built from the mask arithmetic -- a kept child is
    /// worth one payload bit per tile, a handed on child is worth
    /// nothing -- and the encoding is built by walking tiles and
    /// skipping the ones inside a handed on child. They come from
    /// different code, so if a binding ever paid for a tile it gave
    /// away they would not agree.
    #[test]
    fn a_binding_writes_nothing_for_what_it_hands_on() {
        let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
        let mut out = Encoded::default();
        let mut masked = 0;
        for masking in Masking::ALL {
            for (case, bits) in cases().iter().enumerate() {
                pyramid.clear();
                pyramid.rebuild(bits);
                encode(&pyramid, bits, masking, &mut work, &mut out);
                assert_eq!(
                    out.counts.accounted,
                    out.bits(),
                    "masking {}, case {case}: the bits written and the bits accounted for differ",
                    masking.name()
                );
                masked += out.counts.masked_bindings;
            }
        }
        // And the check is not vacuous: bindings that hand children
        // on do happen in these cases.
        assert!(masked > 0, "no binding in the whole corpus handed a child on");
    }

    /// Forbidding a mask cannot lose a cell, only bits. Every
    /// threshold is round tripped above; this holds the other half:
    /// that a stricter rule never comes out smaller, since all it
    /// does is take an option away.
    #[test]
    fn forbidding_a_mask_never_makes_an_encoding_smaller() {
        let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
        let mut out = Encoded::default();
        for bits in cases().iter().take(12) {
            pyramid.clear();
            pyramid.rebuild(bits);
            let mut last = 0;
            for masking in Masking::ALL {
                encode(&pyramid, bits, masking, &mut work, &mut out);
                assert!(
                    out.bits() >= last,
                    "masking {} came out smaller than a looser rule",
                    masking.name()
                );
                last = out.bits();
            }
        }
    }

    /// A workspace holds the last bitmap's survey, so an encode must
    /// leave nothing of it readable.
    #[test]
    fn a_reused_workspace_does_not_leak_the_last_bitmap() {
        let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
        let (mut out, mut back) = (Encoded::default(), BitMatrix::new());
        for bits in [checkerboard(1), BitMatrix::new(), checkerboard(3)] {
            pyramid.clear();
            pyramid.rebuild(&bits);
            encode(&pyramid, &bits, Masking::Anywhere, &mut work, &mut out);
            decode(&out, &mut back);
            assert_eq!(bits.count_set(), back.count_set());
        }
    }
}
