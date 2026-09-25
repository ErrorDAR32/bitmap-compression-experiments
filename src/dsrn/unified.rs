//! One descent, one code table: binding, copying and splitting decided
//! region by region rather than tile size by tile size.
//!
//! The tile passes in [`passes`](super::passes) sweep the whole
//! quadtree once per tile size, and a region that wants to copy has to
//! wait for the size passes to finish before the 1x1 pass reaches it.
//! That is the wrong shape for what the encoding is actually doing.
//!
//! Delta encoding pays off because a region can be described by one
//! payload bit per tile, which is as cheap as description gets. What
//! is left over is the heterogeneous part, which is expensive to
//! describe as tree and expensive to describe as 2x2 payloads, and
//! which copying handles well. So the two want to be decided together,
//! per region: capture as much homogeneous area as binding can, and
//! hand the rest to copying.
//!
//! # The codes
//!
//! Every node of the quadtree writes two bits, and the decoder always
//! knows which region it is at, so nothing else has to be said about
//! where it is or how big it is.
//!
//! | code | meaning | what follows |
//! |------|---------|--------------|
//! | `00` | split | the four children, in reading order |
//! | `01` | bind | the tile depth, then one payload bit per tile |
//! | `10` | copy | a direction |
//! | `11` | masked | a four bit quadrant mask, then a copy or a binding for the quadrants it covers |
//!
//! The tile depth is how many levels below the region the tiles are,
//! written in unary: depth 0 is the region itself, one payload bit for
//! the whole of it. So a region of side 128 whose 2x2 squares are each
//! homogeneous binds at depth 6 and spends 4096 payload bits plus nine
//! bits of header, where a plain quadtree would spend 5461 nodes.
//!
//! # Masking
//!
//! A mask is how a description leaves a hole in itself. The four bits
//! say which quadrants the description covers; the quadrants it does
//! not cover follow as children, and each of those may mask in turn.
//! That is what makes the payload of a region exclude the bits a
//! nested region would have needed: a large region full of small
//! heterogeneous patches binds over everything but the patches, and
//! the patches describe themselves.
//!
//! Masking works the same way over a copy, which is where it earns the
//! most: a region that matches its neighbour everywhere except one
//! corner copies the other three quadrants for eleven bits.

use crate::dsrn::region::{alike, copy_in, settle, settled, Region, CHILDREN, DIRECTIONS};
use crate::dsrn::stream::Bits;
use crate::dsrn::{Pyramid, LEVELS};
use crate::BitMatrix;

/// The two bit code every node writes.
const SPLIT: u64 = 0b00;
const BIND: u64 = 0b01;
const COPY: u64 = 0b10;
const MASKED: u64 = 0b11;

/// What a mask covers: a copy from a neighbour, or a binding.
const OF_COPY: u64 = 0;
const OF_BIND: u64 = 1;

/// The widths the costs are counted in.
const CODE: usize = 2;
const DIRECTION: usize = 2;
const MASK: usize = 4;
const KIND: usize = 1;

/// A tile depth written in unary: `depth` ones and a nought.
const fn unary(depth: usize) -> usize {
    depth + 1
}

/// Tiles in a region bound at `depth`.
const fn tiles(depth: usize) -> usize {
    1 << (2 * depth)
}

/// What a homogeneous region costs: the code, a depth of nought, and
/// the one bit that says what it holds.
const UNIT: u32 = (CODE + unary(0) + 1) as u32;

/// How many of each code an encode wrote.
#[derive(Default, Clone, Copy)]
pub struct Counts {
    pub splits: usize,
    pub binds: usize,
    pub copies: usize,
    pub masked_copies: usize,
    pub masked_binds: usize,
    /// Payload bits that went to regions the masks left holes in.
    pub masked_tiles: usize,
}

/// What an encode produces.
#[derive(Default)]
pub struct Encoded {
    pub counts: Counts,
    /// The codes, masks, directions and depths.
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
pub struct Work {
    /// Per node, the shallowest depth at which every tile of it is
    /// homogeneous. Level 0 is never stored: a cell is always
    /// homogeneous.
    depth: [Box<[u8]>; LEVELS + 1],
    /// Per node, the cheapest description of it that does not copy.
    cost: [Box<[u32]>; LEVELS + 1],
    /// Which cells the decoder will already have, and so may copy
    /// from. Both halves keep it the same way.
    settled: BitMatrix,
}

impl Default for Work {
    fn default() -> Self {
        Self::new()
    }
}

impl Work {
    /// A workspace with its room already found: a byte and a word per
    /// node of every level but the cells.
    pub fn new() -> Self {
        let nodes = |level: usize| Pyramid::side(level) * Pyramid::side(level);
        Self {
            depth: std::array::from_fn(|level| vec![0u8; nodes(level.max(1))].into_boxed_slice()),
            cost: std::array::from_fn(|level| vec![0u32; nodes(level.max(1))].into_boxed_slice()),
            settled: BitMatrix::new(),
        }
    }

    /// Where a node sits in its level's array.
    fn at(level: usize, x: usize, y: usize) -> usize {
        y * Pyramid::side(level) + x
    }

    /// The shallowest depth at which every tile of a node is
    /// homogeneous, as [`survey`] left it.
    fn depth_of(&self, region: Region) -> usize {
        if region.level == 0 {
            return 0;
        }
        self.depth[region.level][Self::at(region.level, region.x, region.y)] as usize
    }

    /// The cheapest description of a node that does not copy, as
    /// [`survey`] left it.
    fn cost_of(&self, region: Region) -> u32 {
        if region.level == 0 {
            return UNIT;
        }
        self.cost[region.level][Self::at(region.level, region.x, region.y)]
    }
}

/// The four children of a region, in reading order.
fn children_of(region: Region) -> [Region; 4] {
    CHILDREN.map(|(dx, dy)| Region {
        level: region.level - 1,
        x: region.x * 2 + dx,
        y: region.y * 2 + dy,
    })
}

/// Whether a region is the same as one of the neighbours it could
/// copy from -- whether or not the decoder will have that neighbour
/// by the time it arrives.
///
/// Which neighbours are settled depends on what every region above
/// this one chose, which the survey runs before. So it asks the
/// weaker question. The answer only ever makes a region look cheaper
/// than it is, so a region that splits on the strength of it still
/// comes back whole; it just spends a few bits more than the survey
/// promised.
fn could_copy(bits: &BitMatrix, region: Region) -> bool {
    let across = Pyramid::side(region.level) as isize;
    DIRECTIONS.iter().any(|&(dx, dy)| {
        let (nx, ny) = (region.x as isize + dx, region.y as isize + dy);
        nx >= 0
            && ny >= 0
            && nx < across
            && ny < across
            && alike(bits, region.level, (region.x, region.y), (nx, ny))
    })
}

/// Reads the pyramid bottom up, leaving every node its tile depth and
/// what it would cost to describe.
///
/// It stops at a homogeneous node rather than walking through it: that
/// node costs [`UNIT`] whatever is beneath it, and the descent never
/// asks about anything beneath it. On a bitmap with large plain areas
/// that is most of the tree unvisited, and on a bitmap with none it
/// stops at the 2x2 squares instead, whose children are cells and so
/// need no visit either.
///
/// Because a node that stops is never split into, its children keep
/// whatever the last bitmap left in them, and nothing may read those.
fn survey(work: &mut Work, pyramid: &Pyramid, bits: &BitMatrix, region: Region) -> (u8, u32) {
    let (depth, cost) = if pyramid.at(region.level, region.x, region.y).is_some() {
        // Nothing beats one code, a depth of nought and one bit.
        (0, UNIT)
    } else {
        let (depth, plain) = if region.level == 1 {
            // Its four tiles are cells, so they are homogeneous and it
            // binds at depth one. Splitting would spend four codes to
            // say the same four bits.
            (1, (CODE + unary(1) + tiles(1)) as u32)
        } else {
            let (mut deepest, mut split) = (0u8, CODE as u32);
            for child in children_of(region) {
                let (depth, cost) = survey(work, pyramid, bits, child);
                deepest = deepest.max(depth);
                split += cost;
            }
            let depth = deepest + 1;
            let bind = (CODE + unary(depth as usize) + tiles(depth as usize)) as u32;
            (depth, bind.min(split))
        };
        let copy = (CODE + DIRECTION) as u32;
        (depth, if plain > copy && could_copy(bits, region) { copy } else { plain })
    };
    let at = Work::at(region.level, region.x, region.y);
    work.depth[region.level][at] = depth;
    work.cost[region.level][at] = cost;
    (depth, cost)
}

/// What a region's tiles hold, at a depth below it.
fn tile_value(pyramid: &Pyramid, bits: &BitMatrix, region: Region, depth: usize, at: usize) -> bool {
    let across = 1usize << depth;
    let (level, tx, ty) =
        (region.level - depth, region.x * across + at % across, region.y * across + at / across);
    if level == 0 {
        bits.get(tx as u8, ty as u8)
    } else {
        pyramid.value(level, tx, ty)
    }
}

/// Writes a region's tiles, one bit each, in reading order.
fn bind_out(
    pyramid: &Pyramid,
    bits: &BitMatrix,
    region: Region,
    depth: usize,
    out: &mut Encoded,
) {
    for at in 0..tiles(depth) {
        out.payload.push(tile_value(pyramid, bits, region, depth, at) as u64, 1);
    }
}

/// Writes a tile depth in unary.
fn depth_out(depth: usize, out: &mut Encoded) {
    if depth > 0 {
        out.tree.push((1u64 << depth) - 1, depth);
    }
    out.tree.push(0, 1);
}

/// Which quadrants of a region match the same quadrants of the
/// neighbour in a direction, and are settled there.
fn mask_of(bits: &BitMatrix, done: &BitMatrix, region: Region, (dx, dy): (isize, isize)) -> u64 {
    let mut mask = 0;
    for (bit, child) in children_of(region).into_iter().enumerate() {
        let (nx, ny) = (child.x as isize + 2 * dx, child.y as isize + 2 * dy);
        if settled(done, child.level, nx, ny)
            && alike(bits, child.level, (child.x, child.y), (nx, ny))
        {
            mask |= 1 << bit;
        }
    }
    mask
}

/// Encodes the bitmap. The pyramid must already hold it.
pub fn encode(
    pyramid: &Pyramid,
    bits: &BitMatrix,
    work: &mut Work,
    out: &mut Encoded,
) {
    out.clear();
    work.settled.words.fill(0);
    let whole = Region { level: LEVELS, x: 0, y: 0 };
    survey(work, pyramid, bits, whole);
    write(work, pyramid, bits, whole, out);
}

/// Describes one region, and whatever of it the description leaves
/// out.
fn write(work: &mut Work, pyramid: &Pyramid, bits: &BitMatrix, region: Region, out: &mut Encoded) {
    let depth = work.depth_of(region);

    // A homogeneous region is done in four bits, which is as cheap as
    // any code gets, so nothing else is worth asking. This is also
    // what keeps the descent out of the part of the tree the survey
    // pruned: the survey stopped here, so this region's children hold
    // nothing anyone may read.
    if depth == 0 {
        out.counts.binds += 1;
        out.tree.push(BIND, CODE);
        depth_out(0, out);
        bind_out(pyramid, bits, region, 0, out);
        settle(&mut work.settled, region);
        return;
    }

    // Binding is tried first, and wins ties: capturing homogeneous
    // area outright never costs a neighbour's luck, and leaves that
    // luck for a region with nothing else to spend.
    let bind = CODE + unary(depth) + tiles(depth);
    let (mut best, mut how) = (bind, Chosen::Bind(depth));

    {
        let split = CODE + children_of(region).iter().map(|&c| work.cost_of(c) as usize).sum::<usize>();
        if split < best {
            (best, how) = (split, Chosen::Split);
        }
    }

    for (dir, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
        let (nx, ny) = (region.x as isize + dx, region.y as isize + dy);
        if settled(&work.settled, region.level, nx, ny)
            && alike(bits, region.level, (region.x, region.y), (nx, ny))
        {
            if CODE + DIRECTION < best {
                (best, how) = (CODE + DIRECTION, Chosen::Copy(dir));
            }
            break;
        }
    }

    if region.level > 0 {
        let children = children_of(region);
        let outside = |mask: u64| -> usize {
            (0..4).filter(|bit| mask >> bit & 1 == 0).map(|bit| work.cost_of(children[bit]) as usize).sum()
        };

        // A masked copy: the quadrants that match a neighbour are
        // taken from it, and the rest describe themselves.
        for (dir, &step) in DIRECTIONS.iter().enumerate() {
            let mask = mask_of(bits, &work.settled, region, step);
            if mask == 0 || mask == 0b1111 {
                continue;
            }
            let cost = CODE + MASK + KIND + DIRECTION + outside(mask);
            if cost < best {
                (best, how) = (cost, Chosen::MaskedCopy(mask, dir));
            }
        }

        // A masked binding: the quadrants shallow enough to bind at
        // some depth are bound, and the deeper ones describe
        // themselves. The depths worth trying are the ones that take
        // in one more quadrant than the last.
        for child in children {
            let depth = work.depth_of(child) + 1;
            if depth > region.level {
                continue;
            }
            let mut mask = 0u64;
            for (bit, &child) in children.iter().enumerate() {
                if work.depth_of(child) < depth {
                    mask |= 1 << bit;
                }
            }
            if mask == 0b1111 {
                continue;
            }
            let held = (mask.count_ones() as usize) * tiles(depth - 1);
            let cost = CODE + MASK + KIND + unary(depth) + held + outside(mask);
            if cost < best {
                (best, how) = (cost, Chosen::MaskedBind(mask, depth));
            }
        }
    }

    match how {
        Chosen::Bind(depth) => {
            out.counts.binds += 1;
            out.tree.push(BIND, CODE);
            depth_out(depth, out);
            bind_out(pyramid, bits, region, depth, out);
            settle(&mut work.settled, region);
        }
        Chosen::Copy(dir) => {
            out.counts.copies += 1;
            out.tree.push(COPY, CODE);
            out.tree.push(dir as u64, DIRECTION);
            settle(&mut work.settled, region);
        }
        Chosen::Split => {
            out.counts.splits += 1;
            out.tree.push(SPLIT, CODE);
            for child in children_of(region) {
                write(work, pyramid, bits, child, out);
            }
        }
        Chosen::MaskedCopy(mask, dir) | Chosen::MaskedBind(mask, dir) => {
            let copying = matches!(how, Chosen::MaskedCopy(..));
            if copying {
                out.counts.masked_copies += 1;
            } else {
                out.counts.masked_binds += 1;
            }
            out.tree.push(MASKED, CODE);
            out.tree.push(mask, MASK);
            out.tree.push(if copying { OF_COPY } else { OF_BIND }, KIND);
            if copying {
                out.tree.push(dir as u64, DIRECTION);
            } else {
                depth_out(dir, out);
            }

            // Everything the mask covers is settled before anything it
            // does not is described, so a hole may copy from the
            // quadrants around it.
            let children = children_of(region);
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 1 {
                    if !copying {
                        out.counts.masked_tiles += tiles(dir - 1);
                        bind_out(pyramid, bits, child, dir - 1, out);
                    }
                    settle(&mut work.settled, child);
                }
            }
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 0 {
                    write(work, pyramid, bits, child, out);
                }
            }
        }
    }
}

/// What [`write`] settled on, and the one number that goes with it: a
/// direction for a copy, a tile depth for a binding.
#[derive(Clone, Copy)]
enum Chosen {
    Bind(usize),
    Copy(usize),
    Split,
    MaskedCopy(u64, usize),
    MaskedBind(u64, usize),
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

    /// A tile depth, read back from unary.
    fn depth(&mut self, out: &Encoded) -> usize {
        let mut depth = 0;
        while self.take(out, 1) == 1 {
            depth += 1;
        }
        depth
    }
}

/// Reads the bitmap back. The workspace need not be the one that
/// encoded it.
pub fn decode(out: &Encoded, work: &mut Work, bits: &mut BitMatrix) {
    bits.words.fill(0);
    let mut reading = Reading::default();
    read(&mut reading, out, work, bits, Region { level: LEVELS, x: 0, y: 0 });
}

/// Puts back one region, and whatever its description left out.
fn read(reading: &mut Reading, out: &Encoded, work: &mut Work, bits: &mut BitMatrix, region: Region) {
    match reading.take(out, CODE) {
        BIND => {
            let depth = reading.depth(out);
            bind_in(reading, out, bits, region, depth);
        }
        COPY => {
            let dir = reading.take(out, DIRECTION) as usize;
            copy_in(bits, region, DIRECTIONS[dir]);
        }
        MASKED => {
            let mask = reading.take(out, MASK);
            let copying = reading.take(out, KIND) == OF_COPY;
            let (dir, depth) = if copying {
                (reading.take(out, DIRECTION) as usize, 0)
            } else {
                (0, reading.depth(out))
            };
            let children = children_of(region);
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 1 {
                    if copying {
                        let (dx, dy) = DIRECTIONS[dir];
                        copy_in(bits, child, (2 * dx, 2 * dy));
                    } else {
                        bind_in(reading, out, bits, child, depth - 1);
                    }
                }
            }
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 0 {
                    read(reading, out, work, bits, child);
                }
            }
        }
        _ => {
            for child in children_of(region) {
                read(reading, out, work, bits, child);
            }
        }
    }
}

/// Puts back a region's tiles from the payload.
fn bind_in(
    reading: &mut Reading,
    out: &Encoded,
    bits: &mut BitMatrix,
    region: Region,
    depth: usize,
) {
    let across = 1usize << depth;
    let side = 1usize << (region.level - depth);
    for at in 0..tiles(depth) {
        let set = out.payload.take(reading.payload, 1).unwrap_or(0) == 1;
        reading.payload += 1;
        if set {
            let (tx, ty) = (region.x * across + at % across, region.y * across + at / across);
            bits.set_rect(
                (tx * side) as i64,
                (ty * side) as i64,
                (tx * side + side - 1) as i64,
                (ty * side + side - 1) as i64,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples;

    /// A checkerboard of squares `side` cells across.
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
        let (mut pyramid, mut work) = (Pyramid::new(), Work::new());
        let (mut out, mut back) = (Encoded::default(), BitMatrix::new());
        for (case, bits) in cases().iter().enumerate() {
            pyramid.clear();
            pyramid.rebuild(bits);
            encode(&pyramid, bits, &mut work, &mut out);
            decode(&out, &mut work, &mut back);
            for y in 0..=u8::MAX {
                for x in 0..=u8::MAX {
                    assert_eq!(
                        bits.get(x, y),
                        back.get(x, y),
                        "case {case} differs at ({x}, {y})"
                    );
                }
            }
        }
    }

    /// A workspace holds the last bitmap's survey where this one's
    /// pruning stopped, so an encode must never read those nodes. The
    /// way to catch it is to encode a plain bitmap after a detailed
    /// one in the same workspace.
    #[test]
    fn a_reused_workspace_does_not_leak_the_last_bitmap() {
        let (mut pyramid, mut work) = (Pyramid::new(), Work::new());
        let (mut out, mut back) = (Encoded::default(), BitMatrix::new());

        pyramid.clear();
        pyramid.rebuild(&checkerboard(1));
        encode(&pyramid, &checkerboard(1), &mut work, &mut out);

        let empty = BitMatrix::new();
        pyramid.clear();
        pyramid.rebuild(&empty);
        encode(&pyramid, &empty, &mut work, &mut out);
        assert_eq!(out.bits(), CODE + unary(0) + 1);

        decode(&out, &mut work, &mut back);
        assert_eq!(back.count_set(), 0);
    }

    /// Every region of a bitmap that holds one thing is homogeneous,
    /// so the whole of it is one code, one depth and one bit.
    #[test]
    fn a_bitmap_of_one_value_is_four_bits() {
        let (mut pyramid, mut work) = (Pyramid::new(), Work::new());
        let mut out = Encoded::default();
        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        for bits in [BitMatrix::new(), full] {
            pyramid.clear();
            pyramid.rebuild(&bits);
            encode(&pyramid, &bits, &mut work, &mut out);
            assert_eq!(out.bits(), 4);
            assert_eq!(out.counts.binds, 1);
        }
    }
}
