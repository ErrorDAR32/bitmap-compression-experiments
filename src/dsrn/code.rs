//! The tile passes: the tree delta, the payload, and reading them back.
//!
//! A **region** is a square of the quadtree. A **tile** is a square of
//! the size the pass is working. A pass asks of each region whether
//! the tiles inside it are homogeneous -- all of them, some of them, or
//! none -- and labels the region by the answer. A region that binds
//! emits one value per tile. A pass ends when every region left is
//! deferred, and those carry into the next pass at the next size down.
//!
//! A region is never smaller than the tile: a region of exactly the
//! tile's size holds one tile, so for it "some" cannot happen.
//!
//! # What the labels do
//!
//! The four codes are the specification's. What the decoder does with
//! them is fixed, and is all it needs to know:
//!
//! | code | what becomes of the region |
//! |---|---|
//! | `01` bind | emits payload, and is finished |
//! | `10` defer | carries into the next pass unchanged |
//! | `00` skip | is dropped, and never described |
//! | `11` subdivide | follows a label, and carries the region's four children instead of the region |
//!
//! A subdivide needs no skip before it and costs two bits: no label is
//! `11`, so `11` where a label would be can only be a subdivide of the
//! region just labelled.
//!
//! Where the children go is the label's to say, and the two are not
//! the same move. A region skipped is done with this tile size, so its
//! children are asked at this size, in this pass. A region deferred is
//! asked again at the next size down, so its children go to the next
//! pass. Feeding both to the same pass makes the two rulesets below
//! one ruleset with two spellings, which is what they were until this
//! was written down.
//!
//! # Which label to give
//!
//! That is the encoder's to choose and the decoder never learns it, so
//! it is a [`Ruleset`] rather than a rule, and they can be measured
//! against each other.
//!
//! # Not here yet
//!
//! The 1x1 pass. What the tile passes leave goes out as raw cells, so
//! the encoding is whole and checkable while the copy codes are still
//! to come.

use crate::dsrn::Pyramid;
use crate::BitMatrix;

/// The labels a region is given, and the subdivision that may follow
/// one. Two bits each.
const SKIP: u64 = 0b00;
const BIND: u64 = 0b01;
const DEFER: u64 = 0b10;
const SUBDIVIDE: u64 = 0b11;

/// The largest tile, and the smallest the tile passes reach.
const TOP: usize = 8;
const BOTTOM: usize = 1;

/// How a region is labelled, given how many of its tiles are
/// homogeneous. The decoder never sees this: it reads the labels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ruleset {
    /// All homogeneous binds, none defers, some skips and subdivides.
    SkipWhereMixed,
    /// All homogeneous binds, some defers and subdivides, none skips.
    ///
    /// Dropping a region whose tiles are all heterogeneous drops
    /// content no later pass will describe, so this one does not come
    /// back the bitmap that went in. It is here to be measured, not to
    /// be used.
    SkipWhereHeterogeneous,
    /// All homogeneous binds, some defers and subdivides, none defers.
    DeferWhereHeterogeneous,
    /// All homogeneous binds, and anything else skips and subdivides
    /// as far as it can before deferring. Refines a region at the tile
    /// size it already failed at, rather than waiting for the next one.
    SubdivideWhereNotAll,
}

impl Ruleset {
    /// Every ruleset, for measuring one against another.
    pub const ALL: [Ruleset; 4] = [
        Ruleset::SkipWhereMixed,
        Ruleset::SkipWhereHeterogeneous,
        Ruleset::DeferWhereHeterogeneous,
        Ruleset::SubdivideWhereNotAll,
    ];

    /// What to call this ruleset in a table.
    pub fn name(self) -> &'static str {
        match self {
            Ruleset::SkipWhereMixed => "skip where mixed",
            Ruleset::SkipWhereHeterogeneous => "skip where heterogeneous",
            Ruleset::DeferWhereHeterogeneous => "defer where heterogeneous",
            Ruleset::SubdivideWhereNotAll => "subdivide where not all",
        }
    }

    /// The label for a region, and whether it subdivides.
    ///
    /// `all` is set when every tile in the region is homogeneous and
    /// `any` when at least one is, so the three cases are `all`,
    /// `any` without `all`, and neither.
    fn label(self, all: bool, any: bool, may_subdivide: bool) -> (u64, bool) {
        match self {
            _ if all => (BIND, false),
            Ruleset::SkipWhereMixed if any => (SKIP, may_subdivide),
            Ruleset::SkipWhereMixed => (DEFER, false),
            Ruleset::SkipWhereHeterogeneous if any => (DEFER, may_subdivide),
            Ruleset::SkipWhereHeterogeneous => (SKIP, false),
            Ruleset::DeferWhereHeterogeneous if any => (DEFER, may_subdivide),
            Ruleset::DeferWhereHeterogeneous => (DEFER, false),
            Ruleset::SubdivideWhereNotAll if may_subdivide => (SKIP, true),
            Ruleset::SubdivideWhereNotAll => (DEFER, false),
        }
    }
}

/// A square of the quadtree: side `1 << level`, at `(x, y)` in units
/// of that side.
#[derive(Clone, Copy)]
struct Region {
    level: usize,
    x: usize,
    y: usize,
}

/// A stream of bits, written low end first.
#[derive(Default)]
pub struct Bits {
    words: Vec<u64>,
    len: usize,
}

impl Bits {
    /// How many bits have been written.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether nothing has been written.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn clear(&mut self) {
        self.words.clear();
        self.len = 0;
    }

    fn push(&mut self, value: u64, width: usize) {
        let (at, shift) = (self.len / 64, self.len % 64);
        if at >= self.words.len() {
            self.words.push(0);
        }
        self.words[at] |= value << shift;
        if shift + width > 64 {
            self.words.push(value >> (64 - shift));
        }
        self.len += width;
    }

    /// `width` bits from `at`, or `None` past the end of the stream.
    fn take(&self, at: usize, width: usize) -> Option<u64> {
        if at + width > self.len {
            return None;
        }
        let (word, shift) = (at / 64, at % 64);
        let mask = if width == 64 { u64::MAX } else { (1u64 << width) - 1 };
        let mut got = self.words[word] >> shift;
        if shift + width > 64 {
            got |= self.words[word + 1] << (64 - shift);
        }
        Some(got & mask)
    }
}

/// What an encode produces: the tree delta, and the payload in the
/// order the regions were bound.
#[derive(Default)]
pub struct Encoded {
    pub tree: Bits,
    pub payload: Bits,
    /// The cells the tile passes did not describe, as raw bits. Stands
    /// in for the 1x1 pass.
    pub leftover: Bits,
}

impl Encoded {
    /// Every bit of the encoding.
    pub fn bits(&self) -> usize {
        self.tree.len() + self.payload.len() + self.leftover.len()
    }

    fn clear(&mut self) {
        self.tree.clear();
        self.payload.clear();
        self.leftover.clear();
    }
}

/// The room an encode works in, found once and reused.
#[derive(Default)]
pub struct Work {
    stack: Vec<Region>,
    deferred: Vec<Region>,
    next: Vec<Region>,
}

/// The four children of a region, in the order a pass takes them:
/// top left, top right, bottom left, bottom right.
const CHILDREN: [(usize, usize); 4] = [(0, 0), (1, 0), (0, 1), (1, 1)];

fn children_of(region: Region) -> [Region; 4] {
    CHILDREN.map(|(dx, dy)| Region {
        level: region.level - 1,
        x: region.x * 2 + dx,
        y: region.y * 2 + dy,
    })
}

/// Encodes the bitmap. The pyramid must already hold it.
pub fn encode(
    pyramid: &Pyramid,
    bits: &BitMatrix,
    rule: Ruleset,
    work: &mut Work,
    out: &mut Encoded,
) {
    out.clear();
    work.deferred.clear();
    work.deferred.push(Region { level: TOP, x: 0, y: 0 });

    for tile in (BOTTOM..=TOP).rev() {
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
            let (all, any) = pyramid.block(tile, tx, ty, across);
            let (label, subdivide) = rule.label(all, any, region.level > tile);

            out.tree.push(label, 2);
            if subdivide {
                out.tree.push(SUBDIVIDE, 2);
            }

            if label == BIND {
                for row in ty..ty + across {
                    for col in tx..tx + across {
                        out.payload.push(u64::from(pyramid.value(tile, col, row)), 1);
                    }
                }
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
    // region no tile size described. This is where the 1x1 pass
    // belongs; until it exists the cells go out raw.
    for region in &work.deferred {
        let side = 1 << region.level;
        for y in region.y * side..(region.y + 1) * side {
            for x in region.x * side..(region.x + 1) * side {
                out.leftover.push(u64::from(bits.get(x as u8, y as u8)), 1);
            }
        }
    }
}

/// Reads an encoding back into a bitmap. Follows the labels, so it
/// needs to know nothing about which [`Ruleset`] wrote them.
pub fn decode(out: &Encoded, work: &mut Work, bits: &mut BitMatrix) {
    bits.words.fill(0);
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
            let Some(label) = out.tree.take(tree_at, 2) else { break };
            tree_at += 2;
            // No label is `11`, so `11` here can only be this region
            // subdividing rather than the next region's label.
            let subdivide = out.tree.take(tree_at, 2) == Some(SUBDIVIDE);
            if subdivide {
                tree_at += 2;
            }

            if label == BIND {
                let across = 1 << (region.level - tile);
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

    let mut raw = 0usize;
    for region in &work.deferred {
        let side = 1 << region.level;
        for y in region.y * side..(region.y + 1) * side {
            for x in region.x * side..(region.x + 1) * side {
                if out.leftover.take(raw, 1) == Some(1) {
                    bits.set(x as u8, y as u8);
                }
                raw += 1;
            }
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
        let (mut pyramid, mut work) = (Pyramid::new(), Work::default());
        let (mut out, mut back) = (Encoded::default(), BitMatrix::new());
        for bits in &cases() {
            pyramid.clear();
            pyramid.rebuild(bits);
            encode(&pyramid, bits, rule, &mut work, &mut out);
            decode(&out, &mut work, &mut back);
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

    /// The rulesets that describe every region come back the bitmap
    /// that went in. Nothing else about an encoding matters if this is
    /// ever false.
    #[test]
    fn the_rulesets_that_describe_everything_round_trip() {
        assert!(round_trips(Ruleset::SkipWhereMixed));
        assert!(round_trips(Ruleset::DeferWhereHeterogeneous));
    }

    /// And the one that drops a region does not, which is the whole
    /// reason it is not the default.
    #[test]
    fn skipping_a_heterogeneous_region_loses_the_bitmap() {
        assert!(!round_trips(Ruleset::SkipWhereHeterogeneous));
    }
}
