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
//! 00  bind          a tile size, then a payload bit for every tile
//! 01  subdivide     the four children, in reading order
//! 10  copy          a direction
//! 11  masked copy   a direction and a four bit child mask; the children
//!                   the mask names follow as regions of their own, and
//!                   the rest are copied
//! ```
//!
//! Every field is fixed width once the code and the region are known,
//! so there is exactly one way to read any stream. A binding's tile
//! size field is [`size_width`] bits, which depends only on the
//! region's level, and both halves compute it the same way.
//!
//! # The tile size is not a choice
//!
//! A binding covers its whole region and writes one bit per tile, so
//! every tile of it has to be homogeneous. Coarser than the coarsest
//! size that manages that and a payload bit would be a lie; finer and
//! the same answer goes out four times over. So there is one size a
//! region can name, and one fold up the pyramid finds it for every
//! region at once.
//!
//! Regions are **disjoint**: each describes its own area and nothing
//! else. A binding that could fill most of a region and hand the
//! awkward part to its subtree was built and measured, and it lost --
//! it has to spend a bit on every binding saying whether it did, and
//! only a quarter of them ever do. What it would have reached,
//! subdividing reaches for two bits.
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

/// The two bit code every region writes.
const BIND: u64 = 0b00;
const SUBDIVIDE: u64 = 0b01;
const COPY: u64 = 0b10;
const MASKED_COPY: u64 = 0b11;

/// The widths, in bits, of everything that is not a tile size.
const CODE: usize = 2;
const DIRECTION: usize = 2;
const CHILD_MASK: usize = 4;
/// Tiles in a region whose tiles are `depth` levels below it.
fn tiles(depth: usize) -> usize {
    1 << (2 * depth)
}

/// The width of a binding's tile size field, for a region at `level`.
///
/// A region of side `1 << level` can name `level + 1` tile sizes, the
/// cells up to the region itself, so it spends exactly the bits those
/// need: none at a cell, one at a 2x2, four at the whole bitmap. A
/// flat field would have to be four bits everywhere and would still
/// be a cap if the bitmap ever grew.
fn size_width(level: usize) -> usize {
    let mut width = 0;
    while (1 << width) < level + 1 {
        width += 1;
    }
    width
}

/// What a binding of a region at `level` spends before its payload:
/// the code and the tile size.
pub fn head_of_a_binding(level: usize) -> usize {
    CODE + size_width(level)
}

/// How many of each code an encode wrote.
#[derive(Default, Clone, Copy)]
pub struct Counts {
    pub bindings: usize,
    pub subdivides: usize,
    pub copies: usize,
    pub masked_copies: usize,
    /// Children a masked copy left to describe themselves.
    pub deferred_children: usize,
    /// Regions bound at one cell a tile, and the payload bits that
    /// went out one cell at a time -- the encoding at its floor.
    pub bound_at_cells: usize,
    pub cells_written: usize,
    /// The cells those regions wrote, by the level of the region that
    /// gave up. A large region giving up costs a bit a cell; a small
    /// one costs its head as well, over very few cells.
    pub cells_given_up: [usize; LEVELS + 1],
    /// Of those regions, the ones holding the same cells as a
    /// neighbour they may copy from -- but one the decoder will not
    /// hold by the time it arrives.
    pub copies_just_missed: usize,
    /// And the ones with no matching neighbour at all.
    pub no_neighbour_matched: usize,
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

/// What a region settled on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Says {
    /// Bound at a tile size.
    Bind { depth: usize },
    Subdivide,
    Copy { dir: usize },
    /// Copied but for the children the mask names, which describe
    /// themselves.
    MaskedCopy { dir: usize, deferred: u64 },
}

/// The room an encode works in.
///
/// One entry per region of every level, which is 87381 of them, found
/// once and refilled per bitmap.
pub struct Workspace {
    /// The coarsest tile size at which every tile of a region is
    /// homogeneous, as a depth below the region. A region cannot be
    /// bound by anything at a size coarser than this.
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
fn cells(region: Region) -> (usize, usize, usize, usize) {
    let side = 1usize << region.level;
    (region.x * side, region.y * side, side, side)
}

/// Whether two regions of the same size hold the same cells. A cell
/// at a time, as the statement says.
fn same_cells(bits: &BitMatrix, a: Region, b: Region) -> bool {
    let ((ax, ay, side, _), (bx, by, _, _)) = (cells(a), cells(b));
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
    let (x, y, side, _) = cells(region);
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
    let (x, y, side, _) = cells(region);
    work.written.set_rect(
        x as i64,
        y as i64,
        (x + side - 1) as i64,
        (y + side - 1) as i64,
    );
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

/// What a binding costs: the code, the tile size, and a payload bit
/// for every tile of the region.
fn binding_cost(region: Region, depth: usize) -> usize {
    CODE + size_width(region.level) + tiles(depth)
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
fn survey(work: &mut Workspace, pyramid: &Pyramid, bits: &BitMatrix, region: Region) {
    let at = Workspace::at(region);

    if region.level == 0 {
        work.finest[0][at] = 0;
        work.cost[0][at] = binding_cost(region, 0);
        return;
    }

    if homogeneous(pyramid, bits, region).is_some() {
        work.finest[region.level][at] = 0;
        // Every region under it is homogeneous too, so they are
        // surveyed as well: a masked copy above may hand one of them
        // down, and it has to have a cost.
        for child in children_of(region) {
            survey(work, pyramid, bits, child);
        }
        work.cost[region.level][at] = binding_cost(region, 0);
        return;
    }

    let mut deepest = 0;
    for child in children_of(region) {
        survey(work, pyramid, bits, child);
        deepest = deepest.max(work.finest_of(child));
    }
    work.finest[region.level][at] = (deepest + 1) as u8;

    // Binding, at the one size the region can name.
    let mut best = binding_cost(region, work.finest_of(region));

    // Subdividing: the four children and nothing else.
    let mut subdivide = CODE;
    for child in children_of(region) {
        subdivide += work.cost_of(child);
    }
    best = best.min(subdivide);

    // Copying, and copying all but some children.
    for dir in 0..DIRECTIONS.len() {
        let Some(from) = neighbour(region, dir) else { continue };
        if same_cells(bits, region, from) {
            best = best.min(CODE + DIRECTION);
            continue;
        }
        let mut deferred = 0usize;
        let mut any = false;
        for (child, mirror) in children_of(region).into_iter().zip(children_of(from)) {
            if same_cells(bits, child, mirror) {
                any = true;
            } else {
                deferred += work.cost_of(child);
            }
        }
        if any {
            best = best.min(CODE + DIRECTION + CHILD_MASK + deferred);
        }
    }

    work.cost[region.level][at] = best;
}

/// Encodes the bitmap. The pyramid must already hold it.
pub fn encode(pyramid: &Pyramid, bits: &BitMatrix, work: &mut Workspace, out: &mut Encoded) {
    out.clear();
    work.written.words.fill(0);
    let whole = Region { level: LEVELS, x: 0, y: 0 };
    survey(work, pyramid, bits, whole);
    describe(work, pyramid, bits, whole, out);
}

/// What a region will say, now that the descent knows what is
/// written.
fn decide(work: &Workspace, bits: &BitMatrix, region: Region) -> Says {
    let depth = work.finest_of(region);
    let (mut best, mut says) = (binding_cost(region, depth), Says::Bind { depth });

    if region.level > 0 {
        let mut subdivide = CODE;
        for child in children_of(region) {
            subdivide += work.cost_of(child);
        }
        if subdivide < best {
            (best, says) = (subdivide, Says::Subdivide);
        }

        for dir in 0..DIRECTIONS.len() {
            let Some(from) = neighbour(region, dir) else { continue };
            if already_written(work, from) && same_cells(bits, region, from) {
                if CODE + DIRECTION < best {
                    (best, says) = (CODE + DIRECTION, Says::Copy { dir });
                }
                continue;
            }
            // Part of it may still be there to take.
            let (mut deferred, mut cost, mut any) = (0u64, CODE + DIRECTION + CHILD_MASK, false);
            for (bit, (child, mirror)) in
                children_of(region).into_iter().zip(children_of(from)).enumerate()
            {
                if already_written(work, mirror) && same_cells(bits, child, mirror) {
                    any = true;
                } else {
                    deferred |= 1 << bit;
                    cost += work.cost_of(child);
                }
            }
            if any && cost < best {
                (best, says) = (cost, Says::MaskedCopy { dir, deferred });
            }
        }
    }

    says
}

/// Describes one region, and whatever its description leaves out.
fn describe(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bits: &BitMatrix,
    region: Region,
    out: &mut Encoded,
) {
    match decide(work, bits, region) {
        Says::Bind { depth } => {
            out.counts.bindings += 1;
                    if region.level == depth && region.level > 0 {
                out.counts.bound_at_cells += 1;
                out.counts.cells_given_up[region.level] += tiles(depth);
                // Whether a copy was there to be had and reading order
                // took it away, or there was never one.
                let matched = (0..DIRECTIONS.len()).any(|dir| {
                    neighbour(region, dir).is_some_and(|from| same_cells(bits, region, from))
                });
                if matched {
                    out.counts.copies_just_missed += 1;
                } else {
                    out.counts.no_neighbour_matched += 1;
                }
            }
            out.tree.push(BIND, CODE);
            out.tree.push(depth as u64, size_width(region.level));
            // A payload bit for every tile, in reading order.
            for tile in tiles_of(region, depth) {
                let value = homogeneous(pyramid, bits, tile)
                    .expect("a bound tile is homogeneous, or the binding would be a lie");
                out.payload.push(value as u64, 1);
                if tile.level == 0 {
                    out.counts.cells_written += 1;
                }
                mark_written(work, tile);
            }
        }
        Says::Subdivide => {
            out.counts.subdivides += 1;
            out.tree.push(SUBDIVIDE, CODE);
            for child in children_of(region) {
                describe(work, pyramid, bits, child, out);
            }
        }
        Says::Copy { dir } => {
            out.counts.copies += 1;
            out.tree.push(COPY, CODE);
            out.tree.push(dir as u64, DIRECTION);
            mark_written(work, region);
        }
        Says::MaskedCopy { dir, deferred } => {
            out.counts.masked_copies += 1;
            out.tree.push(MASKED_COPY, CODE);
            out.tree.push(dir as u64, DIRECTION);
            out.tree.push(deferred, CHILD_MASK);
            // What the copy takes is written before what it defers is
            // described, so a deferred child may read beside it.
            for (bit, child) in children_of(region).into_iter().enumerate() {
                if deferred >> bit & 1 == 0 {
                    mark_written(work, child);
                }
            }
            for (bit, child) in children_of(region).into_iter().enumerate() {
                if deferred >> bit & 1 == 1 {
                    out.counts.deferred_children += 1;
                    describe(work, pyramid, bits, child, out);
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

/// Walks an encoding's tree and writes out what each region says,
/// one line a region, indented by how deep it sits.
///
/// It reads the same fields in the same order the decoder does, so a
/// stream it cannot walk is a stream the decoder cannot read either.
pub fn explain(out: &Encoded) -> String {
    let mut said = String::new();
    let mut reading = Reading::default();
    retell(&mut reading, out, Region { level: LEVELS, x: 0, y: 0 }, 0, &mut said);
    said
}

fn retell(
    reading: &mut Reading,
    out: &Encoded,
    region: Region,
    deep: usize,
    said: &mut String,
) {
    let side = 1usize << region.level;
    let (x, y, _, _) = cells(region);
    let where_it_is = format!("{:width$}{side}x{side} at ({x}, {y})", "", width = deep * 2);
    match reading.take(out, CODE) {
        BIND => {
            let depth = reading.take(out, size_width(region.level)) as usize;
            let tile = 1usize << (region.level - depth);
            let filled = tiles(depth);
            for _ in 0..filled {
                reading.value(out);
            }
            said.push_str(&format!(
                "{where_it_is}: bind at {tile}x{tile} tiles, {filled} of them\n"
            ));
        }
        COPY => {
            let dir = reading.take(out, DIRECTION) as usize;
            said.push_str(&format!("{where_it_is}: copy from {}\n", WHENCE[dir]));
        }
        MASKED_COPY => {
            let dir = reading.take(out, DIRECTION) as usize;
            let deferred = reading.take(out, CHILD_MASK);
            said.push_str(&format!(
                "{where_it_is}: copy from {}, but for {} of its children\n",
                WHENCE[dir],
                deferred.count_ones()
            ));
            for (bit, child) in children_of(region).into_iter().enumerate() {
                if deferred >> bit & 1 == 1 {
                    retell(reading, out, child, deep + 1, said);
                }
            }
        }
        _ => {
            said.push_str(&format!("{where_it_is}: subdivide\n"));
            for child in children_of(region) {
                retell(reading, out, child, deep + 1, said);
            }
        }
    }
}

/// The directions a copy may name, in the order [`DIRECTIONS`] has
/// them.
const WHENCE: [&str; 4] = ["the top left", "above", "the top right", "the left"];

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
    let (x, y, side, _) = cells(tile);
    bits.set_rect(x as i64, y as i64, (x + side - 1) as i64, (y + side - 1) as i64);
}

/// Copies one region's cells onto another's, a cell at a time.
fn copy_cells(bits: &mut BitMatrix, to: Region, from: Region) {
    let ((tx, ty, side, _), (fx, fy, _, _)) = (cells(to), cells(from));
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
    match reading.take(out, CODE) {
        BIND => {
            let depth = reading.take(out, size_width(region.level)) as usize;
            for tile in tiles_of(region, depth) {
                let value = reading.value(out);
                fill(bits, tile, value);
            }
        }
        COPY => {
            let dir = reading.take(out, DIRECTION) as usize;
            let from = neighbour(region, dir).expect("a copy names a neighbour on the bitmap");
            copy_cells(bits, region, from);
        }
        MASKED_COPY => {
            let dir = reading.take(out, DIRECTION) as usize;
            let deferred = reading.take(out, CHILD_MASK);
            let from = neighbour(region, dir).expect("a copy names a neighbour on the bitmap");
            for (bit, (child, mirror)) in
                children_of(region).into_iter().zip(children_of(from)).enumerate()
            {
                if deferred >> bit & 1 == 0 {
                    copy_cells(bits, child, mirror);
                }
            }
            for (bit, child) in children_of(region).into_iter().enumerate() {
                if deferred >> bit & 1 == 1 {
                    undescribe(reading, out, bits, child);
                }
            }
        }
        _ => {
            for child in children_of(region) {
                undescribe(reading, out, bits, child);
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
        let mut stripes = BitMatrix::new();
        for y in 0..256 {
            if y % 3 == 0 {
                stripes.set_rect(0, y, 255, y);
            }
        }
        cases.push(stripes);
        cases
    }

    /// The encoding comes back the bitmap that went in. Nothing else
    /// about it matters if this is ever false.
    #[test]
    fn the_encoding_comes_back_the_bitmap_that_went_in() {
        let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
        let (mut out, mut back) = (Encoded::default(), BitMatrix::new());
        for (case, bits) in cases().iter().enumerate() {
            pyramid.clear();
            pyramid.rebuild(bits);
            encode(&pyramid, bits, &mut work, &mut out);
            decode(&out, &mut back);
            for y in 0..=u8::MAX {
                for x in 0..=u8::MAX {
                    assert_eq!(bits.get(x, y), back.get(x, y), "case {case} differs at ({x}, {y})");
                }
            }
        }
    }

    /// A bitmap of one value is one binding of one tile: two bits of
    /// code, four of tile size, one saying it does not subdivide, and
    /// one of payload.
    #[test]
    fn a_bitmap_of_one_value_is_one_binding() {
        let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
        let mut out = Encoded::default();
        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        for bits in [BitMatrix::new(), full] {
            pyramid.clear();
            pyramid.rebuild(&bits);
            encode(&pyramid, &bits, &mut work, &mut out);
            assert_eq!(out.counts.bindings, 1);
            assert_eq!(out.bits(), CODE + size_width(LEVELS) + 1);
        }
    }

    /// The tile size field is as wide as the region's own size needs
    /// and no wider.
    #[test]
    fn a_tile_size_field_is_as_wide_as_its_region_needs() {
        for (level, width) in [(0, 0), (1, 1), (2, 2), (3, 2), (4, 3), (7, 3), (8, 4)] {
            assert_eq!(size_width(level), width, "at level {level}");
            assert!(1 << width >= level + 1, "level {level} cannot name all its tile sizes");
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
            encode(&pyramid, &bits, &mut work, &mut out);
            decode(&out, &mut back);
            assert_eq!(bits.count_set(), back.count_set());
        }
    }
}
