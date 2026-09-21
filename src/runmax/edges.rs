//! Which rectangles present a face on each edge line.
//!
//! Both of the later moves ask the same question several times over:
//! what sits against this stretch of this edge? Walking every rectangle
//! to answer it is what the naive version of either move costs, so the
//! faces are indexed by the line they lie on and the answer is two
//! binary searches into a sorted slice.
//!
//! The index is rebuilt whenever the partition changes under it, which
//! is cheap because it is a counting pass rather than a sort.
//!
//! It used to carry a second structure beside the faces: a bitmask per
//! edge line saying which positions any face covered, so that a query
//! about a stretch nothing sits against could be settled without a
//! search. That was worth a fifth of the work when merging swept every
//! area every round, because nearly every query then was about nothing.
//! Once merging started following the cascade instead, the queries it
//! makes are mostly about somewhere it already knows something is, and
//! the mask cost more to keep than it saved: 258.9M instructions to
//! 234.1M when it went, for the same partition to the bit.
//!
//! Worth remembering as a pattern rather than a fact about masks. The
//! mask was measured, was right, and stayed right until the code above
//! it changed -- and nothing about it announced that it had stopped
//! being right.

use crate::data::{bounds, List};
use crate::Area;

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
    pub(crate) fn span(self, r: &Area) -> (u8, u8) {
        match self {
            Axis::Vertical => (r.x0, r.x1),
            Axis::Horizontal => (r.y0, r.y1),
        }
    }

    /// The two lines a neighbour must sit on to touch this rectangle's
    /// faces, `None` where the rectangle is already against the edge of
    /// the matrix.
    pub(crate) fn faces(self, r: &Area) -> [Option<u8>; 2] {
        match self {
            Axis::Vertical => [r.y0.checked_sub(1), r.y1.checked_add(1)],
            Axis::Horizontal => [r.x0.checked_sub(1), r.x1.checked_add(1)],
        }
    }

    /// Grows `taker` to swallow `given`. The two already agree along this
    /// axis and sit against each other across it, so the union is a
    /// rectangle and only the far extents move.
    pub(crate) fn take_in(self, taker: &mut Area, given: &Area) {
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
    pub(crate) area: u32,
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
    /// Every face, grouped by edge line: the faces of line `slot` are
    /// `faces[start[slot] .. start[slot] + len[slot]]`, ordered by
    /// where they start.
    ///
    /// One array rather than a vector per line. There are 1024 lines,
    /// and a vector apiece meant 1024 heap blocks to find, fill, clear
    /// and hold -- for a partition that typically touches a few dozen.
    faces: Box<[Face; bounds::FACES]>,
    /// Where each line's group begins, and how long it is. Two arrays
    /// rather than one running total, so that building the index costs
    /// what the areas cost and not what the matrix costs: a running
    /// total has to be swept across all 1024 lines, and the lines a
    /// partition uses are the only ones that matter.
    ///
    /// That mattered once a preview search started merging twenty areas
    /// eighteen million times. Merging four areas cost the same 1.2us
    /// as merging ten, because the fixed sweep dwarfed both.
    start: Box<[u32; bounds::EDGE_LINES]>,
    len: Box<[u32; bounds::EDGE_LINES]>,
    /// Where the next face of each line goes while one is being built.
    cursor: Box<[u32; bounds::EDGE_LINES]>,
    /// The lines this partition puts anything on, so a rebuild clears
    /// those rather than all of them.
    used: List<u32, { bounds::EDGE_LINES }>,
    /// The areas in order of where their faces start, which is how a
    /// line's group comes out sorted without sorting it.
    order: List<u32, { bounds::AREAS }>,
}

impl Edges {
    /// How many edge lines there are per axis and side.
    const LINES: usize = 256;

    /// Up to this many areas, tracking which lines are used beats
    /// sweeping all of them.
    const SMALL: usize = 256;

    /// A blank index, with room for every line of both axes and both
    /// sides. Built once per workspace and rebuilt in place.
    pub(crate) fn new() -> Self {
        let faces = vec![Face { start: 0, end: 0, area: 0 }; bounds::FACES].into_boxed_slice();
        Self {
            faces: faces.try_into().unwrap_or_else(|_| unreachable!("built with FACES slots")),
            start: Box::new([0; bounds::EDGE_LINES]),
            len: Box::new([0; bounds::EDGE_LINES]),
            cursor: Box::new([0; bounds::EDGE_LINES]),
            used: List::new(),
            order: List::new(),
        }
    }

    /// Where one edge line's group sits: the four `[axis][side]` halves
    /// laid end to end, 256 lines apiece.
    pub(crate) fn slot(axis: usize, side: usize, line: u8) -> usize {
        (axis * 2 + side) * Self::LINES + line as usize
    }

    /// Rebuilds the index.
    ///
    /// Each line's group has to come out ordered by where its faces
    /// start, so that the ones overlapping a stretch are a contiguous
    /// slice. Rather than sort each group, every face is counted first,
    /// the counts are turned into starting offsets, and the areas are
    /// then walked in order of the coordinate in question and dropped
    /// at the next free slot of their line -- which leaves every group
    /// sorted for nothing. Positions only run to 255, so putting the
    /// areas in that order is itself a counting pass rather than a sort.
    ///
    /// Nothing here touches a line the partition does not use, which is
    /// what keeps a small merge cheap.
    pub(crate) fn rebuild(&mut self, areas: &[Area]) {
        // Clear whatever the last build filled, by the same route it
        // was filled: a tracked list if there was one, a sweep if not.
        if self.used.is_empty() {
            self.len.fill(0);
        } else {
            for &line in self.used.iter() {
                self.len[line as usize] = 0;
            }
            self.used.clear();
        }

        // A partition small enough to touch few lines is worth tracking
        // which ones; one large enough to touch nearly all of them is
        // not, and the bookkeeping costs more than the sweep it saves.
        // Measured on the main pipeline: tracking unconditionally was
        // 2.1% dearer over a bitmap of five thousand areas.
        let tracking = areas.len() <= Self::SMALL;

        // How many faces each line gets, and which lines get any.
        //
        // One loop with the test inside it, which reads worse than two
        // loops and measures better: splitting them was 238.7M
        // instructions on the main pipeline against 234.4M here, which
        // is code layout and not arithmetic. Not worth chasing further
        // in either direction.
        for area in areas {
            for slot in Self::slots_of(area) {
                if tracking && self.len[slot] == 0 {
                    self.used.push(slot as u32);
                }
                self.len[slot] += 1;
            }
        }

        // Group starts. The order between groups does not matter, only
        // that each is contiguous and none overlap, so a tracked build
        // lays them out in the order the lines were first seen.
        let mut running = 0;
        if tracking {
            for index in 0..self.used.len() {
                let slot = self.used[index] as usize;
                self.start[slot] = running;
                self.cursor[slot] = running;
                running += self.len[slot];
            }
        } else {
            for slot in 0..bounds::EDGE_LINES {
                if self.len[slot] > 0 {
                    self.start[slot] = running;
                    self.cursor[slot] = running;
                    running += self.len[slot];
                }
            }
        }

        // Faces across a horizontal edge line are keyed by x, faces down
        // a vertical one by y. Walking the areas in that order is what
        // leaves each group sorted by where its faces start.
        for across in [true, false] {
            self.place(areas, across);
        }
    }

    /// The four edge lines an area presents a face on, two per axis.
    fn slots_of(area: &Area) -> [usize; 4] {
        [
            Self::slot(0, 0, area.y1),
            Self::slot(0, 1, area.y0),
            Self::slot(1, 0, area.x1),
            Self::slot(1, 1, area.x0),
        ]
    }

    /// Drops one axis's faces into their lines, in order of where they
    /// start.
    fn place(&mut self, areas: &[Area], across: bool) {
        let key = |a: &Area| if across { a.x0 } else { a.y0 } as usize;

        // Counting sort on the starting coordinate, 256 buckets.
        let mut starts = [0u32; 257];
        for area in areas {
            starts[key(area) + 1] += 1;
        }
        for pos in 1..starts.len() {
            starts[pos] += starts[pos - 1];
        }

        self.order.resize(areas.len(), 0);
        for (index, area) in areas.iter().enumerate() {
            let cursor = &mut starts[key(area)];
            self.order[*cursor as usize] = index as u32;
            *cursor += 1;
        }

        for at in 0..self.order.len() {
            let index = self.order[at];
            let area = areas[index as usize];
            let face = if across {
                Face { start: area.x0, end: area.x1, area: index }
            } else {
                Face { start: area.y0, end: area.y1, area: index }
            };
            let slots = if across {
                [Self::slot(0, 0, area.y1), Self::slot(0, 1, area.y0)]
            } else {
                [Self::slot(1, 0, area.x1), Self::slot(1, 1, area.x0)]
            };
            for slot in slots {
                self.faces[self.cursor[slot] as usize] = face;
                self.cursor[slot] += 1;
            }
        }
    }

    /// The faces in a bucket that overlap `lo..=hi`.
    pub(crate) fn overlapping(&self, axis: Axis, side: usize, line: u8, lo: u8, hi: u8) -> &[Face] {
        let group = self.at(axis, side, line);
        let first = group.partition_point(|f| f.end < lo);
        let last = group.partition_point(|f| f.start <= hi);
        &group[first..last.max(first)]
    }

    /// Every face on one edge line, in order of where it starts.
    pub(crate) fn at(&self, axis: Axis, side: usize, line: u8) -> &[Face] {
        let slot = Self::slot(Self::axis_index(axis), side, line);
        let from = self.start[slot] as usize;
        &self.faces[from..from + self.len[slot] as usize]
    }

    /// The axis as the number [`Edges::slot`] wants.
    pub(crate) fn axis_index(axis: Axis) -> usize {
        match axis {
            Axis::Vertical => 0,
            Axis::Horizontal => 1,
        }
    }
}
