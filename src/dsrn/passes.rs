//! The tile passes and the 1x1 pass: the encoding as specified.
//!
//! A **region** is a square of the quadtree. A **tile** is a square of
//! the size the pass is working. A pass asks of each region whether
//! the tiles inside it are homogeneous -- all of them, some of them, or
//! none -- and labels the region by the answer, which the [`Ruleset`]
//! decides. A region that binds emits one value per tile. A pass ends
//! when every region left is deferred, and those carry into the next
//! pass at the next size down.
//!
//! A region is never smaller than the tile: a region of exactly the
//! tile's size holds one tile, so for it "some" cannot happen.
//!
//! What no tile size describes reaches the 1x1 pass, which tries to
//! copy one of the four neighbours of its own size that the decoder
//! will already have. That pass has its own code table and its own
//! stream.

use crate::dsrn::region::{
    alike, children_of, copies_whole, copy_in, settle, settled, Region, DIRECTIONS,
};
use crate::dsrn::rules::{Ruleset, BIND, DEFER, SKIP, SUBDIVIDE};
use crate::dsrn::stream::Bits;
use crate::dsrn::{Pyramid, TileSizedHomogeneity};
use crate::BitMatrix;

/// The largest tile, and the smallest the tile passes reach.
const TOP: usize = 8;
const BOTTOM: usize = 1;

/// How many of each label an encode wrote, for asking where the tree
/// delta's bits go.
#[derive(Default, Clone, Copy)]
pub struct Labels {
    pub bind: usize,
    pub defer: usize,
    pub skip: usize,
    pub subdivide: usize,
    /// Skips that were a copy rather than a subdivision.
    pub copied: usize,
    /// What the 1x1 pass emitted: whole copies, partial copies with a
    /// mask, splits, and regions written out raw.
    pub whole_copies: usize,
    pub part_copies: usize,
    pub splits: usize,
    pub raws: usize,
}

/// What an encode produces: the tree delta, and the payload in the
/// order the regions were bound.
#[derive(Default)]
pub struct Encoded {
    /// What the tree delta is made of.
    pub labels: Labels,
    pub tree: Bits,
    /// The 1x1 pass's codes, which are a different table from the tile
    /// passes' and so are a different stream.
    ///
    /// Sharing one stream desynchronises the decoder: a tile pass
    /// peeks the two bits after a label to see whether a subdivide
    /// follows, and the 1x1 pass's first code can be `11` as well. The
    /// tile decoder eats it, reads a split where a region cannot
    /// split, and descends past the bottom.
    pub copy: Bits,
    pub payload: Bits,
    /// The cells the tile passes did not describe, as raw bits. Stands
    /// in for the 1x1 pass.
    pub leftover: Bits,
    /// How many regions of each side the tile passes left, which is
    /// what the 1x1 pass would be given.
    pub leftover_sides: [usize; 9],
}

impl Encoded {
    /// Every bit of the encoding.
    pub fn bits(&self) -> usize {
        self.tree.len() + self.copy.len() + self.payload.len() + self.leftover.len()
    }

    fn clear(&mut self) {
        self.labels = Labels::default();
        self.tree.clear();
        self.copy.clear();
        self.payload.clear();
        self.leftover.clear();
        self.leftover_sides = [0; 9];
    }
}

/// The room an encode works in, found once and reused.
#[derive(Default)]
pub struct Workspace {
    stack: Vec<Region>,
    deferred: Vec<Region>,
    next: Vec<Region>,
    tiles_homogeneous: TileSizedHomogeneity,
    /// Which cells a binding or an earlier region has already settled,
    /// and so may be copied from. Both halves keep it the same way.
    settled: BitMatrix,
}

/// Encodes the bitmap. The pyramid must already hold it.
pub fn encode(
    pyramid: &Pyramid,
    bits: &BitMatrix,
    rule: Ruleset,
    work: &mut Workspace,
    out: &mut Encoded,
) {
    out.clear();
    work.deferred.clear();
    work.deferred.push(Region { level: TOP, x: 0, y: 0 });
    // Nothing is settled until something describes it.
    work.settled.words.fill(0);

    for tile in (BOTTOM..=TOP).rev() {
        // Every region's answer for this tile size at once, folded up
        // from the children rather than scanned per region.
        work.tiles_homogeneous.rebuild(pyramid, tile);
        work.stack.clear();
        // Depth first, so the stack is filled back to front and the
        // regions come off it in reading order.
        for &region in work.deferred.iter().rev() {
            work.stack.push(region);
        }
        work.next.clear();

        while let Some(region) = work.stack.pop() {
            let across = 1 << (region.level - tile);
            let (tx, ty) = (region.x * across, region.y * across);

            // A region matching a neighbour already settled is
            // described by that neighbour, which is what skip means.
            if rule.may_copy(across * across) {
                if let Some(dir) = copies_whole(bits, &work.settled, region) {
                    out.tree.push(SKIP, 2);
                    out.tree.push(1, 1);
                    out.tree.push(dir as u64, 2);
                    out.labels.skip += 1;
                    out.labels.copied += 1;
                    settle(&mut work.settled, region);
                    continue;
                }
            }

            let (all, any) = work.tiles_homogeneous.at(region.level, region.x, region.y);
            let (label, subdivide) = rule.label(all, any, region.level > tile);

            out.tree.push(label, 2);
            match label {
                BIND => out.labels.bind += 1,
                DEFER => out.labels.defer += 1,
                _ => out.labels.skip += 1,
            }
            if label == SKIP && rule.may_copy(across * across) {
                out.tree.push(0, 1);
            }
            if subdivide {
                out.tree.push(SUBDIVIDE, 2);
                out.labels.subdivide += 1;
            }

            if label == BIND {
                for row in ty..ty + across {
                    for col in tx..tx + across {
                        out.payload.push(u64::from(pyramid.value(tile, col, row)), 1);
                    }
                }
                settle(&mut work.settled, region);
            }

            match (label, subdivide) {
                (BIND, _) | (SKIP, false) => {}
                // Skipped: done with this size, so the children are
                // asked at this size, in this pass.
                (SKIP, true) => {
                    for child in children_of(region).iter().rev() {
                        work.stack.push(*child);
                    }
                }
                // Deferred: asked again at the next size down, so the
                // children go to the next pass.
                (DEFER, true) => work.next.extend_from_slice(&children_of(region)),
                (DEFER, false) => work.next.push(region),
                _ => unreachable!("every label is bind, defer or skip"),
            }
        }

        std::mem::swap(&mut work.deferred, &mut work.next);
    }

    // Whatever is still deferred after the smallest tile pass is a
    // region no tile size described, and the 1x1 pass has it. Every
    // cell not in one of those regions is already settled by a
    // binding, and so may be copied from.
    for region in &work.deferred {
        out.leftover_sides[region.level] += 1;
    }
    let regions = std::mem::take(&mut work.deferred);
    for region in &regions {
        describe(bits, &mut work.settled, *region, out);
    }
    work.deferred = regions;
}

/// Reads an encoding back into a bitmap. Follows the labels, so it
/// needs to know nothing about which [`Ruleset`] wrote them.
pub fn decode(out: &Encoded, rule: Ruleset, work: &mut Workspace, bits: &mut BitMatrix) {
    bits.words.fill(0);
    work.settled.words.fill(0);
    work.deferred.clear();
    work.deferred.push(Region { level: TOP, x: 0, y: 0 });
    let (mut tree_at, mut payload_at) = (0usize, 0usize);

    for tile in (BOTTOM..=TOP).rev() {
        work.stack.clear();
        for &region in work.deferred.iter().rev() {
            work.stack.push(region);
        }
        work.next.clear();

        while let Some(region) = work.stack.pop() {
            let across: usize = 1 << (region.level - tile);
            let Some(label) = out.tree.take(tree_at, 2) else { break };
            tree_at += 2;

            // A skip says the region is described by something else,
            // and one bit says by what.
            if label == SKIP && rule.may_copy(across * across) {
                let copied = out.tree.take(tree_at, 1) == Some(1);
                tree_at += 1;
                if copied {
                    let dir = out.tree.take(tree_at, 2).unwrap_or(0) as usize;
                    tree_at += 2;
                    copy_in(bits, region, DIRECTIONS[dir]);
                    settle(&mut work.settled, region);
                    continue;
                }
            }

            // No label is `11`, so `11` here can only be this region
            // subdividing rather than the next region's label.
            let subdivide = out.tree.take(tree_at, 2) == Some(SUBDIVIDE);
            if subdivide {
                tree_at += 2;
            }

            if label == BIND {
                let (tx, ty) = (region.x * across, region.y * across);
                let side = 1 << tile;
                for row in ty..ty + across {
                    for col in tx..tx + across {
                        let held = out.payload.take(payload_at, 1) == Some(1);
                        payload_at += 1;
                        if held {
                            bits.set_rect(
                                (col * side) as i64,
                                (row * side) as i64,
                                (col * side + side - 1) as i64,
                                (row * side + side - 1) as i64,
                            );
                        }
                    }
                }
                settle(&mut work.settled, region);
            }

            match (label, subdivide) {
                (BIND, _) | (SKIP, false) => {}
                (SKIP, true) => {
                    for child in children_of(region).iter().rev() {
                        work.stack.push(*child);
                    }
                }
                (DEFER, true) => work.next.extend_from_slice(&children_of(region)),
                (DEFER, false) => work.next.push(region),
                _ => unreachable!("every label is bind, defer or skip"),
            }
        }
        std::mem::swap(&mut work.deferred, &mut work.next);
    }

    let (mut copy_at, mut raw_at) = (0usize, 0usize);
    let regions = std::mem::take(&mut work.deferred);
    for region in &regions {
        undescribe(out, &mut work.settled, *region, bits, &mut copy_at, &mut raw_at);
    }
    work.deferred = regions;
}

/// The 1x1 pass has codes of its own, two bits each.
const COPY: u64 = 0b00;
const COPY_PART: u64 = 0b01;
const RAW: u64 = 0b10;
const SPLIT: u64 = 0b11;

/// Describes one region of the 1x1 pass, and its children if it splits.
///
/// Four codes. A region that matches a settled neighbour of its own
/// size copies it whole. One whose children do not all match may still
/// copy some of them from one direction, which is the masked copy. One
/// that can copy nothing splits, and its children are described in
/// turn. One that can copy nothing and cannot split emits its cells.
///
/// A region of two cells a side cannot split, so it copies whole or
/// emits raw, which is the specification's note that a 2x2 only ever
/// carries a direction.
fn describe(bits: &BitMatrix, done: &mut BitMatrix, region: Region, out: &mut Encoded) {
    if let Some(dir) = copies_whole(bits, done, region) {
        out.copy.push(COPY, 2);
        out.copy.push(dir as u64, 2);
        out.labels.whole_copies += 1;
        settle(done, region);
        return;
    }

    if region.level == BOTTOM {
        out.copy.push(RAW, 2);
        out.labels.raws += 1;
        let side = 1usize << region.level;
        for y in region.y * side..(region.y + 1) * side {
            for x in region.x * side..(region.x + 1) * side {
                out.leftover.push(u64::from(bits.get(x as u8, y as u8)), 1);
            }
        }
        settle(done, region);
        return;
    }

    // Some children may copy from one shared direction. The direction
    // that carries the most of them is the one worth naming.
    let kids = children_of(region);
    let mut best: Option<(usize, u8)> = None;
    for (dir, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
        let mut mask = 0u8;
        for (slot, kid) in kids.iter().enumerate() {
            let (nx, ny) = (kid.x as isize + dx, kid.y as isize + dy);
            if settled(done, kid.level, nx, ny)
                && alike(bits, kid.level, (kid.x, kid.y), (nx, ny))
            {
                mask |= 1 << slot;
            }
        }
        if mask != 0 && best.is_none_or(|(_, had)| mask.count_ones() > had.count_ones()) {
            best = Some((dir, mask));
        }
    }

    match best {
        Some((dir, mask)) => {
            out.labels.part_copies += 1;
            out.copy.push(COPY_PART, 2);
            out.copy.push(dir as u64, 2);
            out.copy.push(mask as u64, 4);
            for (slot, kid) in kids.iter().enumerate() {
                if mask >> slot & 1 != 0 {
                    settle(done, *kid);
                } else {
                    describe(bits, done, *kid, out);
                }
            }
        }
        None => {
            out.labels.splits += 1;
            out.copy.push(SPLIT, 2);
            for kid in kids.iter() {
                describe(bits, done, *kid, out);
            }
        }
    }
}

/// Reads back what [`describe`] wrote.
fn undescribe(
    out: &Encoded,
    done: &mut BitMatrix,
    region: Region,
    bits: &mut BitMatrix,
    tree_at: &mut usize,
    raw_at: &mut usize,
) {
    // A region of the smallest size cannot split, whatever the stream
    // says, so a desynchronised stream stops here rather than
    // descending past the bottom.
    let code = match out.copy.take(*tree_at, 2) {
        Some(code) if region.level > BOTTOM || code == COPY => code,
        _ => RAW,
    };
    *tree_at += 2;
    match code {
        COPY => {
            let dir = out.copy.take(*tree_at, 2).unwrap_or(0) as usize;
            *tree_at += 2;
            copy_in(bits, region, DIRECTIONS[dir]);
            settle(done, region);
        }
        COPY_PART => {
            let dir = out.copy.take(*tree_at, 2).unwrap_or(0) as usize;
            *tree_at += 2;
            let mask = out.copy.take(*tree_at, 4).unwrap_or(0) as u8;
            *tree_at += 4;
            for (slot, kid) in children_of(region).iter().enumerate() {
                if mask >> slot & 1 != 0 {
                    copy_in(bits, *kid, DIRECTIONS[dir]);
                    settle(done, *kid);
                } else {
                    undescribe(out, done, *kid, bits, tree_at, raw_at);
                }
            }
        }
        SPLIT => {
            for kid in children_of(region).iter() {
                undescribe(out, done, *kid, bits, tree_at, raw_at);
            }
        }
        _ => {
            let side = 1usize << region.level;
            for y in region.y * side..(region.y + 1) * side {
                for x in region.x * side..(region.x + 1) * side {
                    if out.leftover.take(*raw_at, 1) == Some(1) {
                        bits.set(x as u8, y as u8);
                    }
                    *raw_at += 1;
                }
            }
            settle(done, region);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples;

    fn cases() -> Vec<BitMatrix> {
        let mut cases = vec![BitMatrix::new()];
        for shape in samples::SHAPES {
            cases.extend(shape.tested());
        }
        cases.extend(samples::grown(0, 1.0, 0.0, 1));
        cases
    }

    /// Whether a ruleset's encoding comes back the bitmap that went in.
    fn round_trips(rule: Ruleset) -> bool {
        let (mut pyramid, mut work) = (Pyramid::new(), Workspace::default());
        let (mut out, mut back) = (Encoded::default(), BitMatrix::new());
        for bits in &cases() {
            pyramid.clear();
            pyramid.rebuild(bits);
            encode(&pyramid, bits, rule, &mut work, &mut out);
            decode(&out, rule, &mut work, &mut back);
            for y in 0..=u8::MAX {
                for x in 0..=u8::MAX {
                    if bits.get(x, y) != back.get(x, y) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Every ruleset comes back the bitmap that went in. Nothing else
    /// about an encoding matters if this is ever false.
    #[test]
    fn every_ruleset_round_trips() {
        for rule in Ruleset::ALL {
            assert!(round_trips(rule), "{} does not", rule);
        }
    }
}
