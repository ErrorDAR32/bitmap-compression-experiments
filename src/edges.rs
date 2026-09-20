//! Which rectangles present a face on each edge line.
//!
//! Both of the later moves ask the same question several times over:
//! what sits against this stretch of this edge? Walking every rectangle
//! to answer it is what the naive version of either move costs, so the
//! faces are indexed by the line they lie on and the answer is two
//! binary searches into a sorted slice.
//!
//! The index is rebuilt whenever the partition changes under it, which
//! is cheap because it is a counting pass rather than a sort, and it is
//! cleared by the slots it filled rather than by walking all thousand.

use crate::bits::{range_mask, LINE_WORDS};
use crate::Rect;

/// Which way a rectangle is cut when it merges. Cutting across its
/// width hands stretches to neighbours above and below; cutting across
/// its height hands them to neighbours left and right.
#[derive(Clone, Copy)]
pub(crate) enum Axis {
    Vertical,
    Horizontal,
}

impl Axis {
    /// The extent a stretch is measured along.
    pub(crate) fn span(self, r: &Rect) -> (u8, u8) {
        match self {
            Axis::Vertical => (r.x0, r.x1),
            Axis::Horizontal => (r.y0, r.y1),
        }
    }

    /// The two lines a neighbour must sit on to touch this rectangle's
    /// faces, `None` where the rectangle is already against the edge of
    /// the matrix.
    pub(crate) fn faces(self, r: &Rect) -> [Option<u8>; 2] {
        match self {
            Axis::Vertical => [r.y0.checked_sub(1), r.y1.checked_add(1)],
            Axis::Horizontal => [r.x0.checked_sub(1), r.x1.checked_add(1)],
        }
    }

    /// Grows `taker` to swallow `given`. The two already agree along this
    /// axis and sit against each other across it, so the union is a
    /// rectangle and only the far extents move.
    pub(crate) fn take_in(self, taker: &mut Rect, given: &Rect) {
        match self {
            Axis::Vertical => {
                taker.y0 = taker.y0.min(given.y0);
                taker.y1 = taker.y1.max(given.y1);
            }
            Axis::Horizontal => {
                taker.x0 = taker.x0.min(given.x0);
                taker.x1 = taker.x1.max(given.x1);
            }
        }
    }
}

/// Rectangle indices bucketed by each of their four edges, so the
/// neighbours sitting against one face of a rectangle are found without
/// scanning the whole list.
/// One rectangle's face on an edge line, carrying the extent it covers
/// so a search through a bucket never has to reach back into the
/// rectangle list. Chasing those indices was most of what the search
/// cost, since each probe landed somewhere else in memory.
#[derive(Clone, Copy)]
pub(crate) struct Face {
    pub(crate) start: u8,
    pub(crate) end: u8,
    pub(crate) rect: u32,
}

/// Which rectangles present a face on each edge line, so that "what
/// sits against this stretch" is a lookup rather than a walk over every
/// rectangle.
///
/// Every merge and every clip asks that question several times, and
/// the answer has to be a contiguous slice ordered by where the faces
/// start, which is what lets an overlapping stretch be found by two
/// binary searches. Rebuilt whenever the partition changes under it.
pub(crate) struct Edges {
    /// Indexed as `[axis][side][line]`: for a vertical cut the sides are
    /// the rectangles whose bottom edge, then whose top edge, lies on
    /// `line`; for a horizontal cut, their right then their left edge.
    buckets: Vec<Vec<Face>>,
    /// Which positions on each edge line any face covers, four words to
    /// a line.
    ///
    /// Most queries ask about a stretch nothing sits against, and on a
    /// bitmap of scattered single cells every query is. Testing the
    /// stretch against this settles those in a few instructions instead
    /// of a search, and searching for nothing was a fifth of the work.
    covered: Vec<u64>,
    /// The slots last filled, so a rebuild clears those rather than
    /// walking all thousand-odd of them. A partition of a few dozen
    /// rectangles touches a few dozen slots.
    filled: Vec<usize>,
    /// Rectangles in order of where their faces start, and the counters
    /// that put them there. See [`Edges::rebuild`].
    order: Vec<u32>,
    counts: Vec<u32>,
}

impl Edges {
    /// How many edge lines there are per axis and side.
    const LINES: usize = 256;

    /// A blank index, with room for every line of both axes and both
    /// sides. Built once per workspace and rebuilt in place.
    pub(crate) fn new() -> Self {
        Self {
            buckets: vec![Vec::new(); 4 * Self::LINES],
            covered: vec![0; 4 * Self::LINES * LINE_WORDS],
            filled: Vec::new(),
            order: Vec::new(),
            counts: Vec::new(),
        }
    }

    /// Where one edge line's bucket sits: the four `[axis][side]`
    /// halves laid end to end, 256 lines apiece.
    pub(crate) fn slot(axis: usize, side: usize, line: u8) -> usize {
        (axis * 2 + side) * Self::LINES + line as usize
    }

    /// Rebuilds the index.
    ///
    /// Each bucket has to come out ordered by where its faces start, so
    /// that the ones overlapping a stretch are a contiguous slice. Rather
    /// than sort each bucket, the rectangles are walked in order of the
    /// coordinate in question and pushed as they come, which leaves every
    /// bucket sorted for free. Positions only run to 255, so that order
    /// is a counting pass rather than a sort.
    pub(crate) fn rebuild(&mut self, rects: &[Rect]) {
        for &slot in &self.filled {
            self.buckets[slot].clear();
            self.covered[slot * LINE_WORDS..(slot + 1) * LINE_WORDS].fill(0);
        }
        self.filled.clear();

        // Faces across a horizontal edge line are keyed by x, faces down
        // a vertical one by y.
        for across in [true, false] {
            self.sort_by_start(rects, across);
            for index in 0..self.order.len() {
                let rect = rects[self.order[index] as usize];
                let face = if across {
                    Face { start: rect.x0, end: rect.x1, rect: self.order[index] }
                } else {
                    Face { start: rect.y0, end: rect.y1, rect: self.order[index] }
                };
                let slots = if across {
                    [Self::slot(0, 0, rect.y1), Self::slot(0, 1, rect.y0)]
                } else {
                    [Self::slot(1, 0, rect.x1), Self::slot(1, 1, rect.x0)]
                };
                for slot in slots {
                    if self.buckets[slot].is_empty() {
                        self.filled.push(slot);
                    }
                    self.buckets[slot].push(face);

                    let first = face.start as usize / 64;
                    let last = face.end as usize / 64;
                    let words = &mut self.covered[slot * LINE_WORDS + first..slot * LINE_WORDS + last + 1];
                    for (offset, word) in words.iter_mut().enumerate() {
                        *word |= range_mask(first + offset, face.start, face.end);
                    }
                }
            }
        }
    }

    /// Fills `order` with the rectangles by where their faces start.
    fn sort_by_start(&mut self, rects: &[Rect], across: bool) {
        let key = |r: &Rect| if across { r.x0 } else { r.y0 } as usize;

        self.counts.clear();
        self.counts.resize(Self::LINES + 1, 0);
        for rect in rects {
            self.counts[key(rect) + 1] += 1;
        }
        for i in 1..self.counts.len() {
            self.counts[i] += self.counts[i - 1];
        }

        self.order.clear();
        self.order.resize(rects.len(), 0);
        for (index, rect) in rects.iter().enumerate() {
            let slot = &mut self.counts[key(rect)];
            self.order[*slot as usize] = index as u32;
            *slot += 1;
        }
    }

    /// The faces in a bucket that overlap `lo..=hi`.
    pub(crate) fn overlapping(&self, axis: Axis, side: usize, line: u8, lo: u8, hi: u8) -> &[Face] {
        let slot = Self::slot(Self::axis_index(axis), side, line);
        let words = &self.covered[slot * LINE_WORDS..(slot + 1) * LINE_WORDS];
        let anything = (lo as usize / 64..=hi as usize / 64)
            .any(|index| words[index] & range_mask(index, lo, hi) != 0);
        if !anything {
            return &[];
        }

        let bucket = self.at(axis, side, line);
        let first = bucket.partition_point(|f| f.end < lo);
        let last = bucket.partition_point(|f| f.start <= hi);
        &bucket[first..last.max(first)]
    }

    /// Every face on one edge line, in order of where it starts.
    pub(crate) fn at(&self, axis: Axis, side: usize, line: u8) -> &[Face] {
        &self.buckets[Self::slot(Self::axis_index(axis), side, line)]
    }

    /// The axis as the number [`Edges::slot`] wants.
    pub(crate) fn axis_index(axis: Axis) -> usize {
        match axis {
            Axis::Vertical => 0,
            Axis::Horizontal => 1,
        }
    }
}
