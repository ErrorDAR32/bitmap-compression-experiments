//! The tree, built and then written. No deltas, no labels relative to
//! a pass: every region says one of four things about itself.
//!
//! # Disjoint, or overlapping with subtree bindings
//!
//! Under [`Overlap::Disjoint`] the regions partition the bitmap. A
//! region either describes itself or hands its whole area to its four
//! children, and nothing describes anything twice, so a binding
//! covers its whole region and every tile of it has to be
//! homogeneous.
//!
//! Under [`Overlap::SubtreeBindings`] a binding may overlap the
//! bindings in its own subtree. It names a tile size coarser than its
//! region can carry, fills the tiles it can, and hands the rest to
//! subtree bindings -- a four bit tile mask says which. Its payload
//! leaves out the bits those would have needed, so a wide plain area
//! with an awkward corner fills three tiles and lets the corner
//! describe itself at whatever size it needs.
//!
//! # The codes
//!
//! | code | meaning | what follows |
//! |------|---------|--------------|
//! | `00` | bind | a tile size, then one payload bit per tile |
//! | `01` | subdivide | the four children, in reading order |
//! | `10` | copy | a direction |
//! | `11` | masked copy | a direction and a four bit tile mask |
//!
//! A **subdivide** says nothing about the bitmap. It is purely
//! structural -- it exists to cut a region into smaller ones further
//! down that do get bound.
//!
//! A **masked copy** copies the tiles its mask leaves clear, and the
//! tiles its mask sets follow as regions of their own: they defer to
//! this region's own encoding rather than to a neighbour.
//!
//! # Tile size
//!
//! A tile size is an aligned size, not a depth: the field names the
//! tile's side outright.
//!
//! Disjoint, the size is forced. A binding writes one bit per tile
//! over its whole region, so every tile has to be homogeneous, and
//! the coarsest such size is one max fold up the pyramid. Nothing to
//! choose, nothing to search.
//!
//! With subtree bindings it becomes a choice, because a size too
//! coarse for part of the region is still usable there -- that part
//! becomes a subtree binding. Every size the region can name is
//! priced and the cheapest taken, which is four stored numbers a
//! size.
//!
//! Three bits name eight sides, 1 to 128, which leaves 256 unnamed:
//! a bitmap that holds one thing throughout binds at 128 and spends
//! four payload bits rather than one. [`Sizing::AsWideAsNeeded`]
//! removes that by spending only as many bits as the region's own
//! size allows, which is fewer than three for a small region and four
//! for the whole bitmap.

use crate::dsrn::copying::{copy_mask, could_copy, matching_tiles, Bound, EVERY_TILE};
use crate::dsrn::region::{children_of, copy_in, row_span, Region, CHILDREN, DIRECTIONS};
use crate::dsrn::stream::Bits;
use crate::dsrn::{Pyramid, LEVELS};
use crate::BitMatrix;

/// Whether a binding may overlap the bindings in its own subtree.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Overlap {
    /// It may not: the regions partition the bitmap, and a binding
    /// covers its whole region.
    Disjoint,
    /// It may: a binding fills the tiles it can and hands the rest to
    /// bindings in its subtree, naming them with a tile mask.
    SubtreeBindings,
}

impl Overlap {
    pub const ALL: [Overlap; 2] = [Overlap::Disjoint, Overlap::SubtreeBindings];

    pub fn name(self) -> &'static str {
        match self {
            Overlap::Disjoint => "disjoint regions",
            Overlap::SubtreeBindings => "bindings may overlap their subtree's",
        }
    }

    /// The bit a binding spends saying whether a tile mask follows.
    fn flag(self) -> usize {
        match self {
            Overlap::Disjoint => 0,
            Overlap::SubtreeBindings => 1,
        }
    }
}

/// The two bit code every region writes.
const BIND: u64 = 0b00;
const SUBDIVIDE: u64 = 0b01;
const COPY: u64 = 0b10;
const MASKED_COPY: u64 = 0b11;

/// The widths the costs are counted in.
const CODE: usize = 2;
const TILE_SIZE: usize = 3;
const DIRECTION: usize = 2;
const TILE_MASK: usize = 4;

/// The largest tile a flat [`TILE_SIZE`] field can name, as a power
/// of two: sides 1 to 128.
const LARGEST_TILE: usize = (1 << TILE_SIZE) - 1;

/// Tiles in a region whose tiles are `depth` levels below it.
const fn tiles(depth: usize) -> usize {
    1 << (2 * depth)
}

/// How a binding says which tile size it uses.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sizing {
    /// A flat field naming the tile's side, 1 to 128.
    Flat,
    /// As many bits as the region's own size allows. A region of side
    /// `1 << level` has `level + 1` tile sizes to choose between, so
    /// it spends no bits at all at the cells and four at the whole
    /// bitmap.
    AsWideAsNeeded,
}

impl Sizing {
    pub const ALL: [Sizing; 2] = [Sizing::Flat, Sizing::AsWideAsNeeded];

    pub fn name(self) -> &'static str {
        match self {
            Sizing::Flat => "three bits of tile size",
            Sizing::AsWideAsNeeded => "as many bits of tile size as the region allows",
        }
    }

    /// The width of the tile size field for a region.
    fn width(self, level: usize) -> usize {
        match self {
            Sizing::Flat => TILE_SIZE,
            // The sizes it could mean are the cells up to the region
            // itself, which is level + 1 of them.
            Sizing::AsWideAsNeeded => usize::BITS as usize - level.leading_zeros() as usize,
        }
    }

    /// The coarsest tile a region may name, as a depth below it.
    fn coarsest(self, level: usize) -> usize {
        match self {
            Sizing::Flat => level.saturating_sub(LARGEST_TILE),
            Sizing::AsWideAsNeeded => 0,
        }
    }
}

/// How many of each code an encode wrote.
#[derive(Default, Clone, Copy)]
pub struct Counts {
    pub bindings: usize,
    pub subdivides: usize,
    pub copies: usize,
    pub masked_copies: usize,
    /// Tiles a masked copy left to a region of their own.
    pub deferred_tiles: usize,
    /// Bindings that handed part of themselves to their subtree.
    pub subtree_bindings: usize,
    /// Tiles those handed down.
    pub tiles_handed_down: usize,
    /// Regions bound at one cell a tile, which is the encoding giving
    /// up and writing the bitmap out.
    pub bound_at_cells: usize,
}

/// What an encode produces.
#[derive(Default)]
pub struct Encoded {
    pub counts: Counts,
    /// The codes, tile sizes, tile masks and directions.
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

/// The room an encode works in, found once and reused.
pub struct Workspace {
    /// Per region, the coarsest tile size that tiles it
    /// homogeneously, as a depth below the region.
    tile_size: [Box<[u8]>; LEVELS + 1],
    /// Per region, the cheapest description of it.
    cost: [Box<[u32]>; LEVELS + 1],
    bound: Bound,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    /// A workspace with its room already found.
    pub fn new() -> Self {
        let regions = |level: usize| Pyramid::side(level) * Pyramid::side(level);
        Self {
            tile_size: std::array::from_fn(|level| {
                vec![0u8; regions(level.max(1))].into_boxed_slice()
            }),
            cost: std::array::from_fn(|level| {
                vec![0u32; regions(level.max(1))].into_boxed_slice()
            }),
            bound: Bound::new(),
        }
    }

    fn at(level: usize, x: usize, y: usize) -> usize {
        y * Pyramid::side(level) + x
    }

    /// The coarsest tile size that tiles a region homogeneously, as
    /// [`tile_sizes`] left it. A binding at a coarser size than this
    /// has tiles that are not homogeneous, and hands those to its
    /// subtree.
    fn tile_size_of(&self, region: Region) -> usize {
        if region.level == 0 {
            return 0;
        }
        self.tile_size[region.level][Self::at(region.level, region.x, region.y)] as usize
    }

    /// The cheapest description of a region, as [`tile_sizes`] left
    /// it.
    fn cost_of(&self, region: Region, sizing: Sizing, overlap: Overlap) -> u32 {
        if region.level == 0 {
            return (CODE + sizing.width(0) + overlap.flag() + 1) as u32;
        }
        self.cost[region.level][Self::at(region.level, region.x, region.y)]
    }
}

/// Which of a region's four tiles one level down need a subtree
/// binding at a tile size: the ones holding a tile of that size that
/// is not homogeneous.
fn needs_subtree(work: &Workspace, region: Region, depth: usize) -> u64 {
    let mut mask = 0;
    for (bit, child) in children_of(region).into_iter().enumerate() {
        if work.tile_size_of(child) > depth - 1 {
            mask |= 1 << bit;
        }
    }
    mask
}

/// What a binding costs: the code, the tile size, the bit saying
/// whether a tile mask follows, the mask, a payload bit for every
/// tile it fills, and a description of each tile it hands to its
/// subtree.
fn binding_cost(
    work: &Workspace,
    region: Region,
    depth: usize,
    sizing: Sizing,
    overlap: Overlap,
) -> usize {
    let head = CODE + sizing.width(region.level) + overlap.flag();
    if overlap == Overlap::Disjoint || depth >= work.tile_size_of(region) {
        // Every tile is homogeneous, so the binding fills all of them
        // and hands its subtree nothing.
        return head + tiles(depth);
    }
    // A region of one tile cannot hand out part of itself: the mask
    // names tiles one level down, and there are none above that.
    if depth == 0 {
        return usize::MAX;
    }
    let mask = needs_subtree(work, region, depth);
    let children = children_of(region);
    let mut cost = head + TILE_MASK;
    for (bit, &child) in children.iter().enumerate() {
        cost += if mask >> bit & 1 == 1 {
            work.cost_of(child, sizing, overlap) as usize
        } else {
            tiles(depth - 1)
        };
    }
    cost
}

/// The tile size a region settles on, and what it costs there.
///
/// Disjoint there is nothing to settle: the coarsest size that tiles
/// the region homogeneously is the only one worth naming. With
/// subtree bindings every size the region can name is priced.
fn cheapest_binding(
    work: &Workspace,
    region: Region,
    sizing: Sizing,
    overlap: Overlap,
) -> (usize, usize) {
    let coarsest = sizing.coarsest(region.level);
    if overlap == Overlap::Disjoint {
        let depth = work.tile_size_of(region).max(coarsest);
        return (depth, binding_cost(work, region, depth, sizing, overlap));
    }
    (coarsest..=region.level)
        .map(|depth| (depth, binding_cost(work, region, depth, sizing, overlap)))
        .min_by_key(|&(_, cost)| cost)
        .unwrap_or((coarsest, usize::MAX))
}

/// Reads the pyramid bottom up, leaving every region the coarsest
/// tile size that tiles it homogeneously, and what it costs to
/// describe.
///
/// The size is a max fold: a homogeneous region is one tile, and any
/// other region is one level finer than its deepest child. Since a
/// binding writes one bit per tile and covers its whole region, every
/// tile has to be homogeneous, so this is not a choice -- anything
/// coarser is wrong and anything finer is the same answer written out
/// four times over.
///
/// It stops at a homogeneous region rather than walking through it,
/// so a region that stops leaves its children holding whatever the
/// last bitmap left there, and nothing may read those.
fn tile_sizes(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bits: &BitMatrix,
    region: Region,
    sizing: Sizing,
    overlap: Overlap,
) -> (u8, u32) {
    if region.level == 0 {
        return (0, work.cost_of(region, sizing, overlap));
    }

    let at = Workspace::at(region.level, region.x, region.y);
    if pyramid.at(region.level, region.x, region.y).is_some() {
        work.tile_size[region.level][at] = 0;
        let (_, cost) = cheapest_binding(work, region, sizing, overlap);
        work.cost[region.level][at] = cost as u32;
        return (0, cost as u32);
    }

    let (mut deepest, mut subdivide) = (0u8, CODE as u32);
    for child in children_of(region) {
        let (depth, cost) = tile_sizes(work, pyramid, bits, child, sizing, overlap);
        deepest = deepest.max(depth);
        subdivide += cost;
    }
    // The coarsest size that tiles the region homogeneously: one
    // level finer than its deepest child's.
    work.tile_size[region.level][at] = deepest + 1;

    let (_, binding) = cheapest_binding(work, region, sizing, overlap);
    let mut cost = (binding as u32).min(subdivide);
    // Which neighbours the decoder will hold depends on what every
    // region above this one chose, which this runs before, so a copy
    // is priced at what it would cost if the neighbour were there.
    // That only ever makes a region look cheaper than it is, so a
    // region that subdivides on the strength of it still comes back
    // whole; it just spends a few bits more than this promised.
    let copy = (CODE + DIRECTION) as u32;
    if cost > copy && could_copy(pyramid, bits, region) {
        cost = copy;
    }

    work.cost[region.level][at] = cost;
    (deepest + 1, cost)
}

/// The tiles of a region at a tile size, as regions.
fn tiles_of(region: Region, depth: usize) -> impl Iterator<Item = Region> {
    let across = 1usize << depth;
    let (level, x, y) = (region.level - depth, region.x * across, region.y * across);
    (0..tiles(depth)).map(move |at| Region { level, x: x + at % across, y: y + at / across })
}

/// Writes a payload bit for every tile of a region, which is a run of
/// the pyramid's value plane, or of the bitmap where the tiles are
/// cells.
fn payload_out(
    pyramid: &Pyramid,
    bits: &BitMatrix,
    region: Region,
    depth: usize,
    out: &mut Encoded,
) {
    let across = 1usize << depth;
    let level = region.level - depth;
    for row in region.y * across..(region.y + 1) * across {
        let mut done = 0;
        while done < across {
            let take = (across - done).min(64);
            let from = region.x * across + done;
            let word = if level == 0 {
                row_span(bits, row, from, take)
            } else {
                pyramid.held_span(level, row, from, take)
            };
            out.payload.push(word, take);
            done += take;
        }
    }
}

/// Encodes the bitmap. The pyramid must already hold it.
pub fn encode(
    pyramid: &Pyramid,
    bits: &BitMatrix,
    sizing: Sizing,
    overlap: Overlap,
    work: &mut Workspace,
    out: &mut Encoded,
) {
    out.clear();
    work.bound.clear();
    let whole = Region { level: LEVELS, x: 0, y: 0 };
    tile_sizes(work, pyramid, bits, whole, sizing, overlap);
    describe(work, pyramid, bits, whole, sizing, overlap, out);
}

/// What the descent settled on for a region.
#[derive(Clone, Copy)]
enum Chosen {
    /// Bound at a tile size, handing the tiles the mask names to its
    /// subtree.
    Bind(usize, u64),
    Subdivide,
    Copy(usize),
    /// Copied but for the tiles the mask sets, which follow as
    /// regions of their own.
    MaskedCopy(u64, usize),
}

/// Describes one region, and whatever its description leaves out.
fn describe(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bits: &BitMatrix,
    region: Region,
    sizing: Sizing,
    overlap: Overlap,
    out: &mut Encoded,
) {
    let (depth, binding) = cheapest_binding(work, region, sizing, overlap);
    let subtrees = if overlap == Overlap::SubtreeBindings && depth < work.tile_size_of(region) {
        needs_subtree(work, region, depth)
    } else {
        0
    };
    let (mut best, mut how) = (binding, Chosen::Bind(depth, subtrees));

    // A homogeneous region never subdivides -- four codes to say one
    // thing four times cannot beat one code saying it once -- and
    // this is also what keeps the descent out of the part of the tree
    // the survey pruned: it stopped here, so this region's children
    // hold nothing anyone may read. A whole copy is still worth
    // asking about, because it reads nothing below the region.
    let plain = pyramid.at(region.level, region.x, region.y).is_some();

    if plain {
        for (dir, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
            let (nx, ny) = (region.x as isize + dx, region.y as isize + dy);
            if CODE + DIRECTION < best
                && work.bound.has(region.level, nx, ny)
                && pyramid.at(region.level, nx as usize, ny as usize)
                    == pyramid.at(region.level, region.x, region.y)
            {
                how = Chosen::Copy(dir);
                break;
            }
        }
    } else if region.level > 0 {
        let children = children_of(region);
        let deferred = |work: &Workspace, mask: u64| -> usize {
            (0..CHILDREN.len())
                .filter(|bit| mask >> bit & 1 == 1)
                .map(|bit| work.cost_of(children[bit], sizing, overlap) as usize)
                .sum()
        };

        let subdivide = CODE + deferred(work, EVERY_TILE);
        if subdivide < best {
            (best, how) = (subdivide, Chosen::Subdivide);
        }

        // The survey already knows which tiles match which neighbour.
        // All that is left is whether the decoder will hold them by
        // the time it arrives, which only the descent can say.
        let matching = copy_mask(pyramid, bits, region);
        for (dir, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
            let mut taken = matching_tiles(matching, dir);
            for (bit, child) in children.iter().enumerate() {
                let (nx, ny) = (child.x as isize + 2 * dx, child.y as isize + 2 * dy);
                if taken >> bit & 1 == 1 && !work.bound.has(child.level, nx, ny) {
                    taken &= !(1 << bit);
                }
            }
            // The mask sets the tiles that are *not* copied.
            let mask = !taken & EVERY_TILE;
            let cost = match mask {
                EVERY_TILE => continue,
                0 => CODE + DIRECTION,
                _ => CODE + DIRECTION + TILE_MASK + deferred(work, mask),
            };
            if cost < best {
                (best, how) =
                    (cost, if mask == 0 { Chosen::Copy(dir) } else { Chosen::MaskedCopy(mask, dir) });
            }
        }
    }

    match how {
        Chosen::Bind(depth, subtrees) => {
            out.counts.bindings += 1;
            if region.level == depth {
                out.counts.bound_at_cells += 1;
            }
            out.tree.push(BIND, CODE);
            // The side of the tile, as a power of two.
            out.tree.push((region.level - depth) as u64, sizing.width(region.level));
            if overlap == Overlap::SubtreeBindings {
                out.tree.push((subtrees != 0) as u64, 1);
            }
            if subtrees == 0 {
                payload_out(pyramid, bits, region, depth, out);
                work.bound.bind(region);
                return;
            }
            out.counts.subtree_bindings += 1;
            out.tree.push(subtrees, TILE_MASK);
            // The tiles the mask leaves clear are filled from here,
            // and are bound before the rest is described so that a
            // subtree binding may read them.
            let children = children_of(region);
            for (bit, &child) in children.iter().enumerate() {
                if subtrees >> bit & 1 == 0 {
                    payload_out(pyramid, bits, child, depth - 1, out);
                    work.bound.bind(child);
                }
            }
            for (bit, &child) in children.iter().enumerate() {
                if subtrees >> bit & 1 == 1 {
                    out.counts.tiles_handed_down += 1;
                    describe(work, pyramid, bits, child, sizing, overlap, out);
                }
            }
        }
        Chosen::Subdivide => {
            out.counts.subdivides += 1;
            out.tree.push(SUBDIVIDE, CODE);
            for child in children_of(region) {
                describe(work, pyramid, bits, child, sizing, overlap, out);
            }
        }
        Chosen::Copy(dir) => {
            out.counts.copies += 1;
            out.tree.push(COPY, CODE);
            out.tree.push(dir as u64, DIRECTION);
            work.bound.bind(region);
        }
        Chosen::MaskedCopy(mask, dir) => {
            out.counts.masked_copies += 1;
            out.tree.push(MASKED_COPY, CODE);
            out.tree.push(dir as u64, DIRECTION);
            out.tree.push(mask, TILE_MASK);
            // Whatever the copy takes is bound before anything it
            // defers is described, so a deferred tile may read the
            // tiles around it.
            let children = children_of(region);
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 0 {
                    work.bound.bind(child);
                }
            }
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 1 {
                    out.counts.deferred_tiles += 1;
                    describe(work, pyramid, bits, child, sizing, overlap, out);
                }
            }
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
pub fn decode(out: &Encoded, sizing: Sizing, overlap: Overlap, bits: &mut BitMatrix) {
    bits.words.fill(0);
    let mut reading = Reading::default();
    undescribe(&mut reading, out, sizing, overlap, bits, Region { level: LEVELS, x: 0, y: 0 });
}

/// Fills in a whole tile.
fn fill(bits: &mut BitMatrix, tile: Region) {
    let side = 1usize << tile.level;
    bits.set_rect(
        (tile.x * side) as i64,
        (tile.y * side) as i64,
        (tile.x * side + side - 1) as i64,
        (tile.y * side + side - 1) as i64,
    );
}

/// Puts back one region, and whatever its description left out.
fn undescribe(
    reading: &mut Reading,
    out: &Encoded,
    sizing: Sizing,
    overlap: Overlap,
    bits: &mut BitMatrix,
    region: Region,
) {
    match reading.take(out, CODE) {
        BIND => {
            let side = reading.take(out, sizing.width(region.level)) as usize;
            let depth = region.level - side.min(region.level);
            let subtrees = overlap == Overlap::SubtreeBindings && reading.take(out, 1) == 1;
            if !subtrees {
                for tile in tiles_of(region, depth) {
                    if reading.value(out) {
                        fill(bits, tile);
                    }
                }
                return;
            }
            let mask = reading.take(out, TILE_MASK);
            let children = children_of(region);
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 0 {
                    for tile in tiles_of(child, depth - 1) {
                        if reading.value(out) {
                            fill(bits, tile);
                        }
                    }
                }
            }
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 1 {
                    undescribe(reading, out, sizing, overlap, bits, child);
                }
            }
        }
        COPY => {
            let dir = reading.take(out, DIRECTION) as usize;
            copy_in(bits, region, DIRECTIONS[dir]);
        }
        MASKED_COPY => {
            let (dx, dy) = DIRECTIONS[reading.take(out, DIRECTION) as usize];
            let mask = reading.take(out, TILE_MASK);
            let children = children_of(region);
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 0 {
                    copy_in(bits, child, (2 * dx, 2 * dy));
                }
            }
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 1 {
                    undescribe(reading, out, sizing, overlap, bits, child);
                }
            }
        }
        _ => {
            for child in children_of(region) {
                undescribe(reading, out, sizing, overlap, bits, child);
            }
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
        for side in [1usize, 2, 3, 8, 16] {
            cases.push(checkerboard(side));
        }
        let mut one = BitMatrix::new();
        one.set(128, 128);
        cases.push(one);
        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        cases.push(full);
        cases
    }

    /// The encoding comes back the bitmap that went in. Nothing else
    /// about it matters if this is ever false.
    #[test]
    fn the_encoding_comes_back_the_bitmap_that_went_in() {
        let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
        let (mut out, mut back) = (Encoded::default(), BitMatrix::new());
        for (sizing, overlap) in Sizing::ALL.into_iter().flat_map(|s| Overlap::ALL.map(|o| (s, o))) {
            for (case, bits) in cases().iter().enumerate() {
                pyramid.clear();
                pyramid.rebuild(bits);
                encode(&pyramid, bits, sizing, overlap, &mut work, &mut out);
                decode(&out, sizing, overlap, &mut back);
                for y in 0..=u8::MAX {
                    for x in 0..=u8::MAX {
                        assert_eq!(
                            bits.get(x, y),
                            back.get(x, y),
                            "{} with {}, case {case}, differs at ({x}, {y})",
                            sizing.name(),
                            overlap.name()
                        );
                    }
                }
            }
        }
    }

    /// A workspace holds the last bitmap's tile sizes where this
    /// one's pruning stopped, so an encode must never read those.
    #[test]
    fn a_reused_workspace_does_not_leak_the_last_bitmap() {
        let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
        let (mut out, mut back) = (Encoded::default(), BitMatrix::new());

        pyramid.clear();
        pyramid.rebuild(&checkerboard(1));
        encode(&pyramid, &checkerboard(1), Sizing::Flat, Overlap::Disjoint, &mut work, &mut out);

        let empty = BitMatrix::new();
        pyramid.clear();
        pyramid.rebuild(&empty);
        encode(&pyramid, &empty, Sizing::Flat, Overlap::Disjoint, &mut work, &mut out);
        assert_eq!(out.counts.bindings, 1);
        assert_eq!(out.counts.subdivides, 0);

        decode(&out, Sizing::Flat, Overlap::Disjoint, &mut back);
        assert_eq!(back.count_set(), 0);
    }

    /// A bitmap that holds one thing is one binding. Three bits of
    /// tile size cannot name a tile of 256, so it binds at 128 and
    /// spends four payload bits; a field sized to the region names
    /// 256 and spends one.
    #[test]
    fn a_bitmap_of_one_value_is_one_binding() {
        let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
        let mut out = Encoded::default();
        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        for bits in [BitMatrix::new(), full] {
            pyramid.clear();
            pyramid.rebuild(&bits);
            encode(&pyramid, &bits, Sizing::Flat, Overlap::Disjoint, &mut work, &mut out);
            assert_eq!(out.counts.bindings, 1);
            assert_eq!(out.bits(), CODE + TILE_SIZE + 4);

            encode(&pyramid, &bits, Sizing::AsWideAsNeeded, Overlap::Disjoint, &mut work, &mut out);
            assert_eq!(out.counts.bindings, 1);
            assert_eq!(out.bits(), CODE + 4 + 1);
        }
    }
}
