//! The Fastile algorithm: rectangle meshing that works entirely on run
//! lists.
//!
//! It buys its speed by paying up front. Reducing the bitmap to runs in
//! both directions costs one pass over the machine words and nothing is
//! decided by it, but every step afterwards works on a few hundred runs
//! rather than 65536 cells. It answers close to the minimum rather than
//! at it: 1.9% over on realistic input, in half the time the exact
//! algorithm takes. See [`crate::exact`] for the one that is exact.
//!
//! Rows and columns are both reduced to runs once, up front. Each step
//! takes the longest run still standing, in either orientation, and sinks
//! it: the rectangle is that whole run, carried down as far as every run
//! crossing it reaches. Both run lists are then updated to exclude what
//! was taken, splitting a run in two where the rectangle cut through it.
//!
//! Two choices earn their keep, and both are choices to look at less.
//!
//! A step takes its seed whole rather than hunting for the best rectangle
//! lying along it. A step could instead cut its seed into several
//! rectangles, trading depth for count; scoring those cuts by area less a
//! fixed charge per rectangle and sweeping the charge from nothing to
//! unbounded improves the answer monotonically as the charge rises, and
//! saturates once it is high enough to forbid cutting at all. Over 1000
//! realistic bitmaps: 107 rectangles taking the single best rectangle per
//! step, 82 taking all the area under the seed in as few rectangles as
//! that needs, 73.7 taking the seed whole. Chasing area is what costs.
//!
//! Looking further ahead does not help either. A step can run the same
//! check on the runs crossing its seed and commit to a rectangle along
//! one of those instead; across fourteen combinations of how to take each
//! and which to prefer, the best managed 74.29 against 74.79 raw, and
//! after the pass that rewrites the finished partition every one of them
//! landed between 72.36 and 72.72. So there is nothing to rank within a
//! step either, and the search inside a step disappears.
//!
//! Nothing here walks cells. A step costs the length of the seed run, not
//! the width of the grid, and there are as many steps as there are
//! rectangles in the answer.

use crate::{BitMatrix, WIDTH};
use std::collections::BinaryHeap;

/// An inclusive axis-aligned rectangle over the matrix's `u8` coordinate
/// space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x0: u8,
    pub y0: u8,
    pub x1: u8,
    pub y1: u8,
}

impl Rect {
    /// A rectangle can span all 256 positions, which does not fit in a
    /// `u8`, so extents are computed one size up.
    pub fn width(&self) -> u16 {
        self.x1 as u16 - self.x0 as u16 + 1
    }

    pub fn height(&self) -> u16 {
        self.y1 as u16 - self.y0 as u16 + 1
    }

    pub fn area(&self) -> u32 {
        self.width() as u32 * self.height() as u32
    }
}

/// One run, as the inclusive positions it covers. Storing the end rather
/// than a length keeps a run spanning all 256 positions representable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    start: u8,
    end: u8,
}

impl Span {
    fn len(&self) -> u16 {
        self.end as u16 - self.start as u16 + 1
    }
}

/// Runs for one orientation. For rows, `lines[y]` holds column spans; for
/// columns, `lines[x]` holds row spans. One is the seed side and the
/// other the crossing side, and they are built by the same code with the
/// coordinates swapped.
struct Runs {
    lines: Vec<Vec<Span>>,
}

/// The position of the next set bit at or after `from`, if any.
fn next_set(words: &[u64], from: usize) -> Option<usize> {
    let mut index = from / 64;
    let mut word = words[index] & (u64::MAX << (from % 64));
    loop {
        if word != 0 {
            return Some(index * 64 + word.trailing_zeros() as usize);
        }
        index += 1;
        word = *words.get(index)?;
    }
}

/// The position of the next clear bit at or after `from`, or the end.
fn next_clear(words: &[u64], from: usize) -> usize {
    let mut index = from / 64;
    let mut word = !words[index] & (u64::MAX << (from % 64));
    loop {
        if word != 0 {
            return index * 64 + word.trailing_zeros() as usize;
        }
        index += 1;
        match words.get(index) {
            Some(&next) => word = !next,
            None => return words.len() * 64,
        }
    }
}

impl Runs {
    /// Reduces the bitmap to runs in both directions in one pass over
    /// the machine words.
    ///
    /// Reading the bitmap a cell at a time costs the same whatever it
    /// holds: 65536 bit tests to find the few hundred runs a real bitmap
    /// has, and the same 65536 for an empty one. Both directions come
    /// out of the words instead.
    ///
    /// Along a row, a run is found by skipping to the next set bit and
    /// then to the next clear one, so the work is one step per run
    /// rather than one per cell. Down a column, a run starts where a row
    /// has a bit its predecessor did not and ends where its successor
    /// drops it, which is two bitwise operations per word of each row
    /// and then one step per run.
    fn of(source: &BitMatrix) -> (Self, Self) {
        let mut rows = Self { lines: vec![Vec::new(); 256] };
        let mut cols = Self { lines: vec![Vec::new(); 256] };

        // Where the column run still open at each position began.
        let mut opened = [0u8; 256];
        let mut above = [0u64; 4];

        for line in 0..=u8::MAX {
            let row = source.row(line);

            let spans = &mut rows.lines[line as usize];
            let mut pos = 0;
            while let Some(start) = next_set(row, pos) {
                let end = next_clear(row, start) - 1;
                spans.push(Span { start: start as u8, end: end as u8 });
                pos = end + 1;
                if pos >= WIDTH {
                    break;
                }
            }

            for (index, (&word, &before)) in row.iter().zip(above.iter()).enumerate() {
                let mut starting = word & !before;
                while starting != 0 {
                    let pos = index * 64 + starting.trailing_zeros() as usize;
                    opened[pos] = line;
                    starting &= starting - 1;
                }

                let mut ending = before & !word;
                while ending != 0 {
                    let pos = index * 64 + ending.trailing_zeros() as usize;
                    cols.lines[pos].push(Span { start: opened[pos], end: line - 1 });
                    ending &= ending - 1;
                }
            }

            above.copy_from_slice(row);
        }

        // Whatever is still open runs to the last line.
        for (index, &word) in above.iter().enumerate() {
            let mut open = word;
            while open != 0 {
                let pos = index * 64 + open.trailing_zeros() as usize;
                cols.lines[pos].push(Span { start: opened[pos], end: u8::MAX });
                open &= open - 1;
            }
        }

        (rows, cols)
    }

    /// The run covering `pos`, if any. Runs on a line are disjoint and
    /// kept in ascending order, so the last one starting at or before
    /// `pos` is the only one that can hold it.
    fn span_at(&self, line: u8, pos: u8) -> Option<Span> {
        let spans = &self.lines[line as usize];
        let idx = spans.partition_point(|s| s.start <= pos);
        let span = *spans.get(idx.checked_sub(1)?)?;
        (pos <= span.end).then_some(span)
    }

    /// Removes `[lo, hi]` from every line in `lines`.
    ///
    /// Because runs on a line are sorted and disjoint, the ones the range
    /// touches form a contiguous stretch found by two binary searches.
    /// Everything strictly inside it is swallowed whole; only the first
    /// can keep a piece on the left and only the last a piece on the
    /// right. Untouched runs are never rewritten, so their index entries
    /// stay valid.
    fn carve(&mut self, lines: (u8, u8), lo: u8, hi: u8, created: &mut Vec<(u8, Span)>) {
        for line in lines.0..=lines.1 {
            let index = line as usize;
            let (first, last, pieces) = {
                let spans = &self.lines[index];
                let first = spans.partition_point(|s| s.end < lo);
                let last = spans.partition_point(|s| s.start <= hi);
                if first >= last {
                    continue;
                }

                let mut pieces: Vec<Span> = Vec::new();
                let head = spans[first];
                if lo > head.start {
                    pieces.push(Span { start: head.start, end: lo - 1 });
                }
                let tail = spans[last - 1];
                if hi < tail.end {
                    pieces.push(Span { start: hi + 1, end: tail.end });
                }
                (first, last, pieces)
            };

            for piece in &pieces {
                created.push((line, *piece));
            }
            self.lines[index].splice(first..last, pieces);
        }
    }
}

/// A lattice point where the region turns through 270 degrees, with
/// three of the four cells around it filled.
///
/// These are the only places a partition is forced to do work: a face
/// holding one is not a rectangle, so every one of them has to have a
/// cut running out of it. How few rectangles the region can be split
/// into is decided entirely by how cheaply they are served, which is
/// what [`crate::exact`] works out and what the Fastile algorithm
/// currently does not look at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Reflex {
    pub x: u8,
    pub y: u8,
}

/// Whether one run on a line covers both `pos - 1` and `pos`.
///
/// The two are adjacent, so a run covering both has to be a single run,
/// and one lookup settles it.
fn spans_the_gap(spans: &[Span], pos: u8) -> bool {
    if pos == 0 {
        return false;
    }
    let index = spans.partition_point(|s| s.start < pos);
    index > 0 && spans[index - 1].end >= pos
}

/// Every reflex corner, found from the row runs.
///
/// Three of four cells filled means exactly one empty, and which one it
/// is says where a run must begin or end. Take the lattice point at
/// `(x, y)`, between rows `y - 1` and `y`:
///
/// - the cell above-left is the empty one exactly when a run in row
///   `y - 1` starts at `x`, and then row `y` must cover `x - 1` and `x`;
/// - above-right, when a run in row `y - 1` ends at `x - 1`, same test;
/// - below-left and below-right are the same two with the rows swapped.
///
/// So the corners are at the ends of runs, and there are two run ends to
/// a run. That makes this a walk over the runs rather than over the
/// cells, and there are a few hundred of the former against 65536 of the
/// latter. Only row runs are needed: a corner is a disagreement between
/// two rows, and the column runs say nothing about it that the rows do
/// not.
///
/// The four cases cannot overlap, since each names a different empty
/// cell and only one cell is empty.
pub fn reflex_corners(source: &BitMatrix) -> Vec<Reflex> {
    let (rows, _) = Runs::of(source);
    let mut corners = Vec::new();
    let empty: Vec<Span> = Vec::new();

    for y in 0..=256usize {
        let above = if y == 0 { &empty } else { &rows.lines[y - 1] };
        let below = if y == 256 { &empty } else { &rows.lines[y] };
        if above.is_empty() && below.is_empty() {
            continue;
        }

        for (ends, opposite) in [(above, below), (below, above)] {
            for span in ends {
                // A run starting at 0, or ending at 255, has the edge of
                // the matrix beyond it, which is a second empty cell.
                if spans_the_gap(opposite, span.start) {
                    corners.push(Reflex { x: span.start, y: y as u8 });
                }
                if span.end < u8::MAX && spans_the_gap(opposite, span.end + 1) {
                    corners.push(Reflex { x: span.end + 1, y: y as u8 });
                }
            }
        }
    }

    corners.sort_unstable();
    corners
}

/// A run ranked as a seed.
///
/// The whole ranking is packed into one integer, most significant field
/// first, so the queue orders seeds with a single comparison rather than
/// walking a chain of fields. That chain is worth removing: on a bitmap
/// of nothing but single cells the queue's own comparisons were 22% of
/// all the work done.
///
/// The packing is also a unique name for the run, since no two runs of
/// the same orientation start at the same cell, so two seeds comparing
/// equal really are the same seed in the same state.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Seed {
    rank: u64,
    line: u8,
    start: u8,
    end: u8,
    is_column: bool,
}

impl Seed {
    /// Bit widths, most significant first: the run's length, then the
    /// area standing in the runs that cross it, then a row run over a
    /// column run, then the upper-left-most, and last whether the area is
    /// the real figure or a ceiling standing in for it.
    fn pack(len: u16, crossing_area: u32, is_column: bool, y: u8, x: u8, counted: bool) -> u64 {
        ((len as u64) << 35)
            | ((crossing_area as u64) << 18)
            | ((!is_column as u64) << 17)
            | ((255 - y as u64) << 9)
            | ((255 - x as u64) << 1)
            | counted as u64
    }

    /// A seed for the queue, with its crossing area counted now if that
    /// is cheap and left at its ceiling if it is not.
    ///
    /// Counting costs a lookup per position of the run. Most runs never
    /// come near the top of the queue, so for a long one the figure is
    /// left at the most it could be and counted only if it gets there;
    /// the ceiling can only overstate a seed, which is what the queue
    /// already tolerates. A single-cell run is the exception: one lookup
    /// settles it, which is cheaper than the extra trip through the queue
    /// that deferring would cost, and a bitmap of nothing but single
    /// cells is the worst case for both.
    fn queued(line: u8, span: Span, is_column: bool, rows: &Runs, cols: &Runs) -> Self {
        if span.len() == 1 {
            return Self::rank(line, span, is_column, rows, cols);
        }
        Self::new(line, span, is_column, span.len() as u32 * 256, false)
    }

    fn rank(line: u8, span: Span, is_column: bool, rows: &Runs, cols: &Runs) -> Self {
        let crossing = if is_column { rows } else { cols };
        let mut crossing_area = 0u32;
        for pos in span.start..=span.end {
            if let Some(run) = crossing.span_at(pos, line) {
                crossing_area += run.len() as u32;
            }
        }
        Self::new(line, span, is_column, crossing_area, true)
    }

    fn new(line: u8, span: Span, is_column: bool, crossing_area: u32, counted: bool) -> Self {
        let (y, x) = if is_column { (span.start, line) } else { (line, span.start) };
        Self {
            rank: Self::pack(span.len(), crossing_area, is_column, y, x, counted),
            line,
            start: span.start,
            end: span.end,
            is_column,
        }
    }

    fn span(&self) -> Span {
        Span { start: self.start, end: self.end }
    }
}

/// The best seed left, or `None` once nothing is standing.
///
/// Carving only shortens runs and only shortens the runs crossing them,
/// so a seed's rank never rises. That makes the queue safe to leave
/// stale: whatever sits on top is ranked at least as high as it deserves,
/// so it is enough to check the run is still there and that its rank has
/// not slipped, and to put it back when it has. A seed whose crossing
/// area was never counted overstates itself the same way, and is counted
/// and put back the first time it reaches the top.
fn best_seed(queue: &mut BinaryHeap<Seed>, rows: &Runs, cols: &Runs) -> Option<Seed> {
    while let Some(seed) = queue.pop() {
        let side = if seed.is_column { cols } else { rows };
        if side.span_at(seed.line, seed.start) != Some(seed.span()) {
            continue;
        }

        let fresh = Seed::rank(seed.line, seed.span(), seed.is_column, rows, cols);
        if fresh == seed {
            return Some(seed);
        }
        queue.push(fresh);
    }

    None
}

/// A [`BitMatrix`] partitioned into rectangles/// A [`BitMatrix`] partitioned into rectangles by repeatedly taking the
/// longest run still standing and sinking it as deep as it will go.
pub struct Fastile {
    rects: Vec<Rect>,
    /// How many of the rectangles are single cells standing alone. They
    /// are kept at the end of the list and never take part in anything.
    alone: usize,
}

impl Fastile {
    pub fn from_bit_matrix(source: &BitMatrix) -> Self {
        // Cells standing alone are forced, so they are set aside rather
        // than queued, seeded, carved and then checked against every
        // neighbour they do not have.
        let (alone, source) = source.split_isolated();
        let source = &source;

        let (mut rows, mut cols) = Runs::of(source);
        let mut queue = BinaryHeap::new();
        for (is_column, side) in [(false, &rows), (true, &cols)] {
            for (line, spans) in side.lines.iter().enumerate() {
                for span in spans {
                    queue.push(Seed::queued(line as u8, *span, is_column, &rows, &cols));
                }
            }
        }

        let mut rects = Vec::new();
        let (mut cut_rows, mut cut_cols) = (Vec::new(), Vec::new());

        while let Some(seed) = best_seed(&mut queue, &rows, &cols) {
            let crossing = if seed.is_column { &rows } else { &cols };
            let rect = sink(crossing, seed.span(), seed.line, seed.is_column);

            cut_rows.clear();
            cut_cols.clear();
            rows.carve((rect.y0, rect.y1), rect.x0, rect.x1, &mut cut_rows);
            cols.carve((rect.x0, rect.x1), rect.y0, rect.y1, &mut cut_cols);
            rects.push(rect);

            // Whatever a carve leaves behind is a seed in its own right,
            // and ranking it needs both sides already updated.
            for (pieces, is_column) in [(&cut_rows, false), (&cut_cols, true)] {
                for &(line, span) in pieces {
                    queue.push(Seed::queued(line, span, is_column, &rows, &cols));
                }
            }
        }

        let mut alone_count = 0;
        alone.for_each_set(|x, y| {
            rects.push(Rect { x0: x, y0: y, x1: x, y1: y });
            alone_count += 1;
        });

        Self { rects, alone: alone_count }
    }

    /// Builds both run lists and answers how many runs there are, which
    /// is the first pass of [`Self::from_bit_matrix`] and nothing else.
    #[doc(hidden)]
    pub fn count_runs(source: &BitMatrix) -> usize {
        let (rows, cols) = Runs::of(source);
        rows.lines.iter().chain(cols.lines.iter()).map(Vec::len).sum()
    }

    pub fn rects(&self) -> &[Rect] {
        &self.rects
    }

    /// Rewrites the partition by giving rectangles away to their
    /// neighbours, and answers how many were reclaimed. See
    /// [`crate::mutate`] for what the moves are and what they cost.
    pub fn compact(&mut self) -> usize {
        self.without_the_alone(crate::mutate::compact)
    }

    /// Only the free half of [`Self::compact`], which reclaims nothing on
    /// its own. Kept so that claim stays measurable.
    #[doc(hidden)]
    pub fn dissolve_only(&mut self) -> usize {
        self.without_the_alone(crate::mutate::dissolve_only)
    }

    /// Runs a rewriting pass over everything but the single cells
    /// standing alone, which nothing can be done with.
    fn without_the_alone(&mut self, pass: impl Fn(&mut Vec<Rect>) -> usize) -> usize {
        let movable = self.rects.len() - self.alone;
        let solitary = self.rects.split_off(movable);
        let reclaimed = pass(&mut self.rects);
        self.rects.extend(solitary);
        reclaimed
    }
}

// ---------------------------------------------------------------------
// Experiment: which way the tie on run length should be settled.
//
// The shipped rule takes the run with the most area in the runs crossing
// it. The worst cases found so far all suggest that is backwards, since a
// run with a lot of area crossing it is a run that severs a lot when it
// is taken.
//
// This scans every run each step instead of using the queue, which is
// slow but exact whichever way the tie goes. The queue only stays sound
// for "most" -- carving can only shrink a crossing run, so a seed's rank
// can fall but never rise, which is what makes a stale entry safe to
// leave on top. Preferring the least reverses that.
// ---------------------------------------------------------------------

impl Fastile {
    #[doc(hidden)]
    pub fn tie_break(source: &BitMatrix, prefer_least: bool) -> Self {
        let (mut rows, mut cols) = Runs::of(source);
        let mut rects = Vec::new();
        let mut sink_bin = Vec::new();

        while let Some(seed) = scan_for_seed(&rows, &cols, prefer_least) {
            let crossing = if seed.is_column { &rows } else { &cols };
            let rect = sink(crossing, seed.span(), seed.line, seed.is_column);
            rows.carve((rect.y0, rect.y1), rect.x0, rect.x1, &mut sink_bin);
            cols.carve((rect.x0, rect.x1), rect.y0, rect.y1, &mut sink_bin);
            sink_bin.clear();
            rects.push(rect);
        }

        Self { rects, alone: 0 }
    }
}

fn scan_for_seed(rows: &Runs, cols: &Runs, prefer_least: bool) -> Option<Seed> {
    /// The seventeen bits the crossing area occupies. Exclusive-oring
    /// them complements the field in place, so a bigger area sorts lower
    /// while every other field keeps its meaning.
    const AREA: u64 = 0x1_FFFF << 18;

    let longest = rows
        .lines
        .iter()
        .chain(cols.lines.iter())
        .flatten()
        .map(Span::len)
        .max()?;

    let mut best: Option<(u64, Seed)> = None;
    for (is_column, side) in [(false, rows), (true, cols)] {
        for (line, spans) in side.lines.iter().enumerate() {
            for span in spans {
                if span.len() != longest {
                    continue;
                }
                let seed = Seed::rank(line as u8, *span, is_column, rows, cols);
                let key = if prefer_least { seed.rank ^ AREA } else { seed.rank };
                if best.is_none_or(|(b, _)| key > b) {
                    best = Some((key, seed));
                }
            }
        }
    }

    best.map(|(_, seed)| seed)
}

/// The whole seed run, taken as far as every one of its crossing runs
/// reaches.
///
/// The seed spans positions `seed.start..=seed.end` on `line`, and each of
/// those positions sits in exactly one crossing run, so the rectangle is
/// as long as the seed and as deep as the shallowest run under it.
fn sink(crossing: &Runs, seed: Span, line: u8, seed_is_column: bool) -> Rect {
    let (mut lo, mut hi) = (0u8, u8::MAX);
    for pos in seed.start..=seed.end {
        let span = crossing
            .span_at(pos, line)
            .expect("a cell still standing belongs to a run of either kind");
        lo = lo.max(span.start);
        hi = hi.min(span.end);
    }

    if seed_is_column {
        Rect { x0: lo, y0: seed.start, x1: hi, y1: seed.end }
    } else {
        Rect { x0: seed.start, y0: lo, x1: seed.end, y1: hi }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// The straightforward way to find runs: look at every cell. Kept
    /// as the reference the fast one is checked against.
    fn runs_cell_by_cell(set: impl Fn(u8, u8) -> bool) -> Runs {
        let mut lines = Vec::with_capacity(256);
        for line in 0..=u8::MAX {
            let mut spans = Vec::new();
            let mut start: Option<u8> = None;
            for pos in 0..=u8::MAX {
                match (set(line, pos), start) {
                    (true, None) => start = Some(pos),
                    (false, Some(s)) => {
                        spans.push(Span { start: s, end: pos - 1 });
                        start = None;
                    }
                    _ => {}
                }
            }
            if let Some(s) = start {
                spans.push(Span { start: s, end: u8::MAX });
            }
            lines.push(spans);
        }
        Runs { lines }
    }

    fn assert_same_runs(bits: &BitMatrix) {
        let (rows, cols) = Runs::of(bits);
        let want_rows = runs_cell_by_cell(|line, pos| bits.get(pos, line));
        let want_cols = runs_cell_by_cell(|line, pos| bits.get(line, pos));
        assert_eq!(rows.lines, want_rows.lines, "row runs differ");
        assert_eq!(cols.lines, want_cols.lines, "column runs differ");
    }

    /// The corners found from the runs have to be the corners found by
    /// looking at every lattice point in turn.
    #[test]
    fn corners_from_runs_agree_with_looking_at_every_point() {
        fn by_inspection(bits: &BitMatrix) -> Vec<Reflex> {
            let filled = |x: i32, y: i32| {
                (0..256).contains(&x) && (0..256).contains(&y) && bits.get(x as u8, y as u8)
            };
            let mut found = Vec::new();
            for y in 0..=256i32 {
                for x in 0..=256i32 {
                    let around = [
                        filled(x - 1, y - 1),
                        filled(x, y - 1),
                        filled(x - 1, y),
                        filled(x, y),
                    ];
                    if around.iter().filter(|q| **q).count() == 3 {
                        found.push(Reflex { x: x as u8, y: y as u8 });
                    }
                }
            }
            found.sort_unstable();
            found
        }

        let mut cases = vec![BitMatrix::new()];

        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        cases.push(full);

        // A plus, which is the smallest shape with reflex corners.
        let mut plus = BitMatrix::new();
        plus.set_rect(1, 0, 1, 2);
        plus.set_rect(0, 1, 2, 1);
        cases.push(plus);

        // A ring, so a hole contributes its corners too.
        let mut ring = BitMatrix::new();
        ring.set_rect(4, 4, 40, 40);
        ring.unset_rect(10, 10, 30, 30);
        cases.push(ring);

        // Shapes pressed against every edge of the matrix.
        for (x0, y0, x1, y1) in [(0, 0, 100, 100), (155, 0, 255, 100), (0, 155, 100, 255)] {
            let mut bits = BitMatrix::new();
            bits.set_rect(x0, y0, x1, y1);
            bits.unset_rect(x0 + 10, y0 + 10, x1, y1);
            cases.push(bits);
        }

        let mut shapes = BitMatrix::new();
        shapes.set_circle(180, 180, 25);
        shapes.unset_circle(180, 180, 8);
        cases.push(shapes);

        let mut checker = BitMatrix::new();
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                if (x as u16 + y as u16).is_multiple_of(2) {
                    checker.set(x, y);
                }
            }
        }
        cases.push(checker);

        let mut seed = 0x9E3779B97F4A7C15u64;
        for _ in 0..30 {
            let mut bits = BitMatrix::new();
            for _ in 0..6 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let x = (seed % 256) as i64;
                let y = ((seed >> 8) % 256) as i64;
                bits.set_rect(x, y, x + (seed >> 16) as i64 % 40, y + (seed >> 24) as i64 % 40);
            }
            cases.push(bits);
        }

        for bits in &cases {
            assert_eq!(reflex_corners(bits), by_inspection(bits));
        }
    }

    /// The word-wise pass has to agree with reading every cell, on
    /// everything from an empty bitmap to a full one, including runs
    /// that end exactly on a word boundary and ones that run to 255.
    #[test]
    fn the_fast_run_pass_agrees_with_reading_every_cell() {
        assert_same_runs(&BitMatrix::new());

        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        assert_same_runs(&full);

        // Word boundaries sit at 64, 128 and 192.
        for edge in [0u8, 1, 63, 64, 65, 127, 128, 191, 192, 254, 255] {
            let mut bits = BitMatrix::new();
            bits.set_rect(0, 0, edge as i64, 255);
            assert_same_runs(&bits);

            let mut bits = BitMatrix::new();
            bits.set_rect(edge as i64, 0, 255, 255);
            assert_same_runs(&bits);

            let mut bits = BitMatrix::new();
            bits.set(edge, edge);
            assert_same_runs(&bits);
        }

        let mut shapes = BitMatrix::new();
        shapes.set_rect(10, 10, 40, 30);
        shapes.set_circle(180, 180, 25);
        shapes.unset_rect(20, 15, 30, 25);
        shapes.unset_circle(180, 180, 8);
        assert_same_runs(&shapes);

        let mut checker = BitMatrix::new();
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                if (x as u16 + y as u16).is_multiple_of(2) {
                    checker.set(x, y);
                }
            }
        }
        assert_same_runs(&checker);

        let mut seed = 0x243F6A8885A308D3u64;
        for _ in 0..40 {
            let mut bits = BitMatrix::new();
            for _ in 0..8 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let x = (seed % 256) as i64;
                let y = ((seed >> 8) % 256) as i64;
                let w = ((seed >> 16) % 40) as i64;
                let h = ((seed >> 24) % 40) as i64;
                bits.set_rect(x, y, x + w, y + h);
            }
            assert_same_runs(&bits);
        }
    }

    fn bits_from_rows(rows: &[&str]) -> BitMatrix {
        let mut bits = BitMatrix::new();
        for (y, row) in rows.iter().enumerate() {
            for (x, cell) in row.bytes().enumerate() {
                if cell == b'#' {
                    bits.set(x as u8, y as u8);
                }
            }
        }
        bits
    }

    /// The invariant that matters: the rectangles cover exactly the set
    /// bits, and never each other.
    ///
    /// Painting them into a matrix and comparing is linear in the grid.
    /// Overlap then falls out of arithmetic rather than comparing every
    /// pair: if the areas sum to more than the cells painted, two
    /// rectangles covered the same cell.
    fn assert_exact_partition(bits: &BitMatrix, mesh: &Fastile) {
        let mut painted = BitMatrix::new();
        for r in mesh.rects() {
            painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
        }

        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                assert_eq!(bits.get(x, y), painted.get(x, y), "mismatch at ({x}, {y})");
            }
        }

        let total: u32 = mesh.rects().iter().map(|r| r.area()).sum();
        assert_eq!(total, painted.count_set(), "rectangles overlap");
    }

    #[test]
    fn empty_and_full() {
        let empty = BitMatrix::new();
        assert_eq!(Fastile::from_bit_matrix(&empty).rects().len(), 0);

        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        let mesh = Fastile::from_bit_matrix(&full);
        assert_eq!(mesh.rects(), &[Rect { x0: 0, y0: 0, x1: 255, y1: 255 }]);
    }

    #[test]
    fn single_rectangle_comes_back_whole() {
        let mut bits = BitMatrix::new();
        bits.set_rect(10, 20, 40, 30);
        let mesh = Fastile::from_bit_matrix(&bits);
        assert_eq!(mesh.rects(), &[Rect { x0: 10, y0: 20, x1: 40, y1: 30 }]);
    }

    #[test]
    fn an_l_splits_into_its_arms() {
        // An "L": a 3-wide top row and a 3-tall left column sharing corner
        // (0,0). The top row is the topmost run and only column 0 carries
        // on below it, so the row comes off flat and the stem is left.
        let mut bits = BitMatrix::new();
        bits.set(0, 0);
        bits.set(1, 0);
        bits.set(2, 0);
        bits.set(0, 1);
        bits.set(0, 2);

        let mesh = Fastile::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(
            mesh.rects(),
            &[
                Rect { x0: 0, y0: 0, x1: 2, y1: 0 },
                Rect { x0: 0, y0: 1, x1: 0, y1: 2 },
            ]
        );
    }

    /// A seed is never cut short to reach deeper. A 6x1 row sits on a 2x2
    /// block, so taking the row whole stops it at depth 1 where taking
    /// only its left half would have carried 2 columns down 3 rows. Both
    /// answers are two rectangles, and refusing to cut is what holds
    /// across the bitmaps where they differ.
    #[test]
    fn the_seed_is_taken_whole_even_where_a_piece_reaches_deeper() {
        let mut bits = BitMatrix::new();
        bits.set_rect(0, 0, 5, 0);
        bits.set_rect(0, 1, 1, 2);

        let mesh = Fastile::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(
            mesh.rects(),
            &[
                Rect { x0: 0, y0: 0, x1: 5, y1: 0 },
                Rect { x0: 0, y0: 1, x1: 1, y1: 2 },
            ]
        );
    }

    /// The worked 8x8 example, where ten rectangles is the proven
    /// optimum. Seeding on the longest run needs eleven; the pass that
    /// rewrites the partition afterwards finds the tenth.
    #[test]
    fn worked_example_reaches_ten_after_compacting() {
        let bits = bits_from_rows(&[
            "####.###", "#..#.###", "####.###", "...#...#", "...##..#", "...#####", "########",
            "##.#####",
        ]);

        let mut mesh = Fastile::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 11);

        mesh.compact();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 10);
    }

    /// The 4x4 whose optimum is 3. Seeding on the longest run and taking
    /// it whole needs 5, and the pass afterwards brings it to the
    /// optimum.
    #[test]
    fn adversarial_four_by_four_is_optimal_after_compacting() {
        let bits = bits_from_rows(&["##..", ".###", "###.", "...."]);

        let mut mesh = Fastile::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 5);

        mesh.compact();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 3);
    }

    #[test]
    fn rects_and_circles_with_holes_punched_out() {
        let mut bits = BitMatrix::new();
        bits.set_rect(10, 10, 40, 30);
        bits.set_circle(180, 180, 25);
        bits.unset_rect(20, 15, 30, 25);
        bits.unset_circle(180, 180, 8);

        let mesh = Fastile::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
    }

    #[test]
    fn small_bitmaps_from_a_fixed_sequence_stay_exact_partitions() {
        let mut seed = 0x243F6A8885A308D3u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };

        for n in [3usize, 4, 5, 6] {
            for _ in 0..150 {
                let mut bits = BitMatrix::new();
                let cells = next();
                for idx in 0..(n * n) {
                    if cells & (1u64 << idx) != 0 {
                        bits.set((idx % n) as u8, (idx / n) as u8);
                    }
                }
                let mesh = Fastile::from_bit_matrix(&bits);
                assert_exact_partition(&bits, &mesh);
            }
        }
    }

    /// Cells standing alone come out as themselves, and being set aside
    /// does not disturb the shape they sit beside.
    #[test]
    fn cells_standing_alone_are_kept_whole() {
        let mut bits = BitMatrix::new();
        bits.set_rect(10, 10, 20, 20);
        for (x, y) in [(0u8, 0u8), (100, 100), (255, 255), (5, 200)] {
            bits.set(x, y);
        }

        let mut mesh = Fastile::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 5, "the block and the four cells");
        assert!(mesh.rects().contains(&Rect { x0: 10, y0: 10, x1: 20, y1: 20 }));

        // The pass has nothing to do with them and must leave them be.
        mesh.compact();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 5);
        for (x, y) in [(0u8, 0u8), (100, 100), (255, 255), (5, 200)] {
            assert!(mesh.rects().contains(&Rect { x0: x, y0: y, x1: x, y1: y }));
        }
    }

    /// A single cell touching something is not standing alone, and still
    /// has to be looked at.
    #[test]
    fn a_single_cell_with_a_neighbour_is_not_set_aside() {
        // An L one cell wide: the corner cell is 1x1 in the answer but
        // every cell here has a neighbour.
        let bits = bits_from_rows(&["##", "#."]);
        let mut mesh = Fastile::from_bit_matrix(&bits);
        mesh.compact();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 2);
    }

    /// The worst case: no two set cells touch, so every run is one cell
    /// long and nothing ever merges.
    #[test]
    fn checkerboard_worst_case() {
        let mut bits = BitMatrix::new();
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                if (x as u16 + y as u16).is_multiple_of(2) {
                    bits.set(x, y);
                }
            }
        }

        let mesh = Fastile::from_bit_matrix(&bits);
        assert_eq!(mesh.rects().len(), 32768);
        assert_exact_partition(&bits, &mesh);
    }
}
