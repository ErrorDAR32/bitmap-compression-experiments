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

/// Which way a tie on run length is settled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tie {
    /// The run with the least area standing in the runs crossing it. A
    /// run with a lot crossing it is a run that severs a lot when it is
    /// taken, so this is the one that leaves the tidier partition.
    Least,
    /// The most, which is what this used to do.
    Most,
}

/// A run waiting to be seeded.
///
/// The ranking packed into `order` is the run's length, then a row run
/// over a column run, then the upper-left-most, so the queue compares
/// seeds with a single instruction rather than walking a chain of
/// fields. The crossing area is deliberately not in it.
///
/// Leaving it out is what makes the queue safe to leave stale. A run's
/// length never changes: carving either takes a run away or leaves it
/// alone, and what it leaves behind is a new run, queued in its own
/// right. So a seed sitting in the queue is either exactly what it says
/// it is or gone, and one lookup tells which. Crossing area is not like
/// that -- it falls as the bitmap is carved -- and a figure that falls
/// cannot be ordered lazily in the direction that prefers it small,
/// because a seed that improved would sit buried under seeds that had
/// not. Keeping it out of the queue and settling it only among the runs
/// actually tied on length sidesteps that entirely, and costs nothing:
/// the tie is the only place it was ever consulted.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Seed {
    order: u32,
    line: u8,
    start: u8,
    end: u8,
    is_column: bool,
}

impl Seed {
    fn new(line: u8, span: Span, is_column: bool) -> Self {
        let (y, x) = if is_column { (span.start, line) } else { (line, span.start) };
        Self {
            order: ((span.len() as u32) << 17)
                | ((!is_column as u32) << 16)
                | ((255 - y as u32) << 8)
                | (255 - x as u32),
            line,
            start: span.start,
            end: span.end,
            is_column,
        }
    }

    fn len(&self) -> u16 {
        self.end as u16 - self.start as u16 + 1
    }

    fn span(&self) -> Span {
        Span { start: self.start, end: self.end }
    }

    /// Whether the run this names is still standing, unchanged.
    fn standing(&self, rows: &Runs, cols: &Runs) -> bool {
        let side = if self.is_column { cols } else { rows };
        side.span_at(self.line, self.start) == Some(self.span())
    }

}

/// How much area stands in the runs crossing a seed.
fn crossing_area(seed: &Seed, rows: &Runs, cols: &Runs) -> u32 {
    let crossing = if seed.is_column { rows } else { cols };
    let mut area = 0;
    for pos in seed.start..=seed.end {
        if let Some(run) = crossing.span_at(pos, seed.line) {
            area += run.len() as u32;
        }
    }
    area
}

/// One run in a level, ranked by its crossing area.
///
/// The area is carried alongside the slot so a ranking that has been
/// superseded can be told from the one in force: they are pushed, never
/// updated in place, and the one whose area still matches the level's is
/// the live one.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Ranked {
    key: u64,
    area: u32,
    slot: u32,
}

/// Marks a run as taken or gone. Real areas never reach it.
const SPENT: u32 = u32::MAX;

/// Marks a run whose area has not been counted yet, and which is
/// standing in the queue at the best figure it could possibly have.
const UNCOUNTED: u32 = u32::MAX - 1;

/// The runs tied at the longest length left, which is the only place the
/// crossing area is consulted.
///
/// A carve can only shorten a run, and every piece it leaves is shorter
/// than the run it came from, so nothing ever joins a level once it is
/// drawn and the longest length only falls.
///
/// Within a level the areas do fall, which is the whole difficulty: a
/// figure that improves cannot be left stale in a queue that prefers it
/// small, because a run that got better would sit buried under runs that
/// had not. So the ones a carve could have reached are recounted at once
/// and pushed again, and what they supersede is recognised on the way
/// out. Finding them is why the runs are bucketed by where they start:
/// a run of this level's length overlaps the carve only if it starts in
/// one stretch of positions, so the buckets to revisit are a range
/// rather than the whole level. Scanning the level instead is what made
/// a bitmap of thousands of equal runs take two thirds of a second.
#[derive(Default)]
struct Level {
    length: u16,
    /// Slot to run and the area in force for it, or [`SPENT`].
    runs: Vec<(Seed, u32)>,
    order: BinaryHeap<Ranked>,
    /// Slots by orientation and by where the run starts.
    buckets: Vec<Vec<u32>>,
    filled: Vec<usize>,
    scratch: Vec<u32>,
}

impl Level {
    const POSITIONS: usize = 256;

    fn new() -> Self {
        Self { buckets: vec![Vec::new(); 2 * Self::POSITIONS], ..Default::default() }
    }

    fn bucket(is_column: bool, start: u8) -> usize {
        usize::from(is_column) * Self::POSITIONS + start as usize
    }

    /// Ranks an area so the better one sorts higher, whichever way the
    /// tie goes. Seventeen bits hold an area, which cannot exceed the
    /// 65536 cells of the matrix.
    fn key(&self, seed: &Seed, area: u32, tie: Tie) -> u64 {
        let by_area = match tie {
            Tie::Least => 0x1_FFFF - area,
            Tie::Most => area,
        };
        ((by_area as u64) << 26) | seed.order as u64
    }

    /// The best figure a run of this length could have, which is one
    /// cell of crossing run per cell of it at the least and the whole
    /// height of the matrix at the most.
    ///
    /// Counting an area costs a lookup per cell of the run, and a level
    /// of long runs all tied is exactly where that is dearest: a solid
    /// square is 512 runs of 256 cells, and counting them all took ten
    /// times what meshing it should. Standing them at their best figure
    /// instead and counting only the ones that reach the top can only
    /// overstate a run, which is what the level already copes with.
    fn bound(&self, tie: Tie) -> u32 {
        match tie {
            Tie::Least => self.length as u32,
            Tie::Most => self.length as u32 * 256,
        }
    }

    /// Draws every run standing at the longest length left.
    fn draw(&mut self, queue: &mut BinaryHeap<Seed>, rows: &Runs, cols: &Runs, tie: Tie) {
        for &slot in &self.filled {
            self.buckets[slot].clear();
        }
        self.filled.clear();
        self.runs.clear();
        self.order.clear();

        self.length = loop {
            match queue.peek() {
                None => return,
                Some(top) if top.standing(rows, cols) => break top.len(),
                Some(_) => drop(queue.pop()),
            }
        };

        while let Some(top) = queue.peek() {
            if top.len() != self.length {
                break;
            }
            let seed = queue.pop().expect("just peeked");
            if !seed.standing(rows, cols) {
                continue;
            }

            let slot = self.runs.len() as u32;
            let key = self.key(&seed, self.bound(tie), tie);
            self.order.push(Ranked { key, area: UNCOUNTED, slot });
            self.runs.push((seed, UNCOUNTED));

            let bucket = Self::bucket(seed.is_column, seed.start);
            if self.buckets[bucket].is_empty() {
                self.filled.push(bucket);
            }
            self.buckets[bucket].push(slot);
        }
    }

    /// The best run left in the level, or `None` once it is exhausted.
    fn take_best(&mut self, rows: &Runs, cols: &Runs, tie: Tie) -> Option<Seed> {
        while let Some(top) = self.order.pop() {
            let slot = top.slot as usize;
            let (seed, area) = self.runs[slot];
            if top.area != area {
                continue;
            }
            if !seed.standing(rows, cols) {
                self.runs[slot].1 = SPENT;
                continue;
            }

            // Standing at its best possible figure, so settle it and let
            // it find its real place.
            if area == UNCOUNTED {
                let counted = crossing_area(&seed, rows, cols);
                self.runs[slot].1 = counted;
                self.order.push(Ranked {
                    key: self.key(&seed, counted, tie),
                    area: counted,
                    slot: top.slot,
                });
                continue;
            }

            self.runs[slot].1 = SPENT;
            return Some(seed);
        }
        None
    }

    /// Recounts the runs the carve could have reached.
    ///
    /// A run of this level's length overlapping `lo..=hi` has to start
    /// somewhere in `lo - (length - 1) ..= hi`, so only those buckets
    /// are visited.
    fn note(&mut self, rect: &Rect, rows: &Runs, cols: &Runs, tie: Tie) {
        for is_column in [false, true] {
            let (lo, hi) = if is_column { (rect.y0, rect.y1) } else { (rect.x0, rect.x1) };
            let first = lo.saturating_sub((self.length - 1) as u8);

            self.scratch.clear();
            for start in first..=hi {
                self.scratch
                    .extend_from_slice(&self.buckets[Self::bucket(is_column, start)]);
            }

            for index in 0..self.scratch.len() {
                let slot = self.scratch[index] as usize;
                let (seed, area) = self.runs[slot];
                // Taken already, or still standing at a figure that
                // cannot be beaten by the area falling further.
                if area == SPENT || area == UNCOUNTED {
                    continue;
                }
                let now = crossing_area(&seed, rows, cols);
                if now != area {
                    self.runs[slot].1 = now;
                    self.order.push(Ranked {
                        key: self.key(&seed, now, tie),
                        area: now,
                        slot: slot as u32,
                    });
                }
            }
        }
    }
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
        Self::with_tie(source, Tie::Least)
    }

    /// The same, with the tie on run length settled either way.
    #[doc(hidden)]
    pub fn with_tie(source: &BitMatrix, tie: Tie) -> Self {
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
                    queue.push(Seed::new(line as u8, *span, is_column));
                }
            }
        }

        let mut level = Level::new();
        let mut rects = Vec::new();
        let (mut cut_rows, mut cut_cols) = (Vec::new(), Vec::new());

        loop {
            let seed = match level.take_best(&rows, &cols, tie) {
                Some(seed) => seed,
                None => {
                    level.draw(&mut queue, &rows, &cols, tie);
                    match level.take_best(&rows, &cols, tie) {
                        Some(seed) => seed,
                        None => break,
                    }
                }
            };

            let crossing = if seed.is_column { &rows } else { &cols };
            let rect = sink(crossing, seed.span(), seed.line, seed.is_column);

            cut_rows.clear();
            cut_cols.clear();
            rows.carve((rect.y0, rect.y1), rect.x0, rect.x1, &mut cut_rows);
            cols.carve((rect.x0, rect.x1), rect.y0, rect.y1, &mut cut_cols);
            level.note(&rect, &rows, &cols, tie);
            rects.push(rect);

            // Whatever a carve leaves behind is a run in its own right,
            // and shorter than the one it came from, so it belongs in the
            // queue rather than the level being worked through.
            for (pieces, is_column) in [(&cut_rows, false), (&cut_cols, true)] {
                for &(line, span) in pieces {
                    queue.push(Seed::new(line, span, is_column));
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
// The same answer, worked out by scanning every run each step instead of
// keeping a queue. Slow, obviously right, and what the fast path is
// checked against.
// ---------------------------------------------------------------------

impl Fastile {
    #[doc(hidden)]
    pub fn by_scanning(source: &BitMatrix, tie: Tie) -> Self {
        let (alone, source) = source.split_isolated();
        let source = &source;

        let (mut rows, mut cols) = Runs::of(source);
        let mut rects = Vec::new();
        let mut bin = Vec::new();

        while let Some(seed) = scan_for_seed(&rows, &cols, tie) {
            let crossing = if seed.is_column { &rows } else { &cols };
            let rect = sink(crossing, seed.span(), seed.line, seed.is_column);
            rows.carve((rect.y0, rect.y1), rect.x0, rect.x1, &mut bin);
            cols.carve((rect.x0, rect.x1), rect.y0, rect.y1, &mut bin);
            bin.clear();
            rects.push(rect);
        }

        let mut alone_count = 0;
        alone.for_each_set(|x, y| {
            rects.push(Rect { x0: x, y0: y, x1: x, y1: y });
            alone_count += 1;
        });

        Self { rects, alone: alone_count }
    }
}

fn scan_for_seed(rows: &Runs, cols: &Runs, tie: Tie) -> Option<Seed> {
    let longest = rows
        .lines
        .iter()
        .chain(cols.lines.iter())
        .flatten()
        .map(Span::len)
        .max()?;

    let mut best: Option<(Seed, u32)> = None;
    for (is_column, side) in [(false, rows), (true, cols)] {
        for (line, spans) in side.lines.iter().enumerate() {
            for span in spans {
                if span.len() != longest {
                    continue;
                }
                let seed = Seed::new(line as u8, *span, is_column);
                let area = crossing_area(&seed, rows, cols);
                let better = match best {
                    None => true,
                    Some((top, top_area)) => match (area == top_area, tie) {
                        (true, _) => seed.order > top.order,
                        (false, Tie::Least) => area < top_area,
                        (false, Tie::Most) => area > top_area,
                    },
                };
                if better {
                    best = Some((seed, area));
                }
            }
        }
    }

    best.map(|(seed, _)| seed)
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

    /// The queue has to reach the same partition as scanning every run
    /// each step, whichever way the tie on length goes. This is the
    /// whole justification for the queue: it is only worth keeping if it
    /// is the same answer, arrived at faster.
    #[test]
    fn the_queue_agrees_with_scanning_every_run() {
        let mut cases = vec![BitMatrix::new()];

        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        cases.push(full);

        let mut plus = BitMatrix::new();
        plus.set_rect(1, 0, 1, 2);
        plus.set_rect(0, 1, 2, 1);
        cases.push(plus);

        let mut ring = BitMatrix::new();
        ring.set_rect(4, 4, 40, 40);
        ring.unset_rect(10, 10, 30, 30);
        cases.push(ring);

        // Ties on length everywhere, which is where the two could differ.
        let mut ladder = BitMatrix::new();
        for row in 0..20 {
            ladder.set_rect(0, row * 3, 9, row * 3);
            ladder.set_rect(row % 10, row * 3 + 1, row % 10, row * 3 + 2);
        }
        cases.push(ladder);

        let mut checker = BitMatrix::new();
        for y in 0..32u8 {
            for x in 0..32u8 {
                if (x + y).is_multiple_of(2) {
                    checker.set(x, y);
                }
            }
        }
        cases.push(checker);

        let mut seed = 0x243F6A8885A308D3u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..60 {
            let mut bits = BitMatrix::new();
            for _ in 0..6 {
                let x = (next() % 60) as i64;
                let y = (next() % 60) as i64;
                bits.set_rect(x, y, x + (next() % 20) as i64, y + (next() % 20) as i64);
            }
            for _ in 0..2 {
                let x = (next() % 60) as i64;
                let y = (next() % 60) as i64;
                bits.unset_rect(x, y, x + (next() % 8) as i64, y + (next() % 8) as i64);
            }
            cases.push(bits);
        }
        // Small dense bitmaps, where ties are thickest.
        for _ in 0..200 {
            let mut bits = BitMatrix::new();
            let cells = next();
            for idx in 0..36 {
                if cells & (1u64 << idx) != 0 {
                    bits.set((idx % 6) as u8, (idx / 6) as u8);
                }
            }
            cases.push(bits);
        }

        for bits in &cases {
            for tie in [Tie::Least, Tie::Most] {
                let quick = Fastile::with_tie(bits, tie);
                let slow = Fastile::by_scanning(bits, tie);
                assert_eq!(
                    quick.rects(),
                    slow.rects(),
                    "the queue and the scan disagree on {tie:?}"
                );
                assert_exact_partition(bits, &quick);
            }
        }
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
    /// optimum. Settling the tie on length by the least crossing area
    /// reaches it outright, where settling it by the most needed eleven
    /// and a rewriting pass afterwards to find the tenth.
    #[test]
    fn worked_example_reaches_ten() {
        let bits = bits_from_rows(&[
            "####.###", "#..#.###", "####.###", "...#...#", "...##..#", "...#####", "########",
            "##.#####",
        ]);

        let mut mesh = Fastile::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 10);

        assert_eq!(Fastile::with_tie(&bits, Tie::Most).rects().len(), 11);

        mesh.compact();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 10, "there is nothing left to find");
    }

    /// The 4x4 whose optimum is 3, which cost five rectangles when the
    /// tie on length went to the most crossing area and needed the pass
    /// to come down. It is reached outright now.
    #[test]
    fn adversarial_four_by_four_is_optimal() {
        let bits = bits_from_rows(&["##..", ".###", "###.", "...."]);

        let mut mesh = Fastile::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 3);

        assert_eq!(Fastile::with_tie(&bits, Tie::Most).rects().len(), 5);

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
