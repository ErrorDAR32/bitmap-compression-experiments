//! Runmax: the mesh, and everything it is built out of.
//!
//! The bitmap is reduced to the cells still standing in both
//! orientations, and every step afterwards works on those rather than
//! on cells. Each step takes the longest run still standing, in either
//! orientation, and covers every cell under it: one rectangle per
//! stretch of the seed whose crossing runs agree, each carried down as
//! far as that run reaches. Both sides are then carved to exclude what
//! was taken.
//!
//! Covering meshes worse than taking the seed whole, deliberately, and
//! that is the point. It lands at 84.27 rectangles per realistic bitmap
//! against 76.06 for taking the seed whole, but the rectangles it
//! leaves are thin, and thin rectangles are the ones [`crate::runmax::grow`]
//! can do something with: 74.66 after the rewriting pass against 75.19.
//!
//! Three structures carry a step. [`Runs`] holds what is standing, as
//! bits. [`Queue`] holds the runs waiting to be seeded, bucketed by
//! length. [`Level`] holds the runs tied at the longest length left,
//! which is the only place the crossing area is ever consulted. Each
//! one is there because a plainer version of it was measured and cost
//! too much; the measurements are in their own docs.

use crate::data::bits::{range_mask, LINE_WORDS};
use crate::data::{Runs, Span};
use crate::{BitMatrix, Rect};
use std::collections::BinaryHeap;

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
pub(crate) struct AreaSeed {
    order: u32,
    pub(crate) line: u8,
    start: u8,
    end: u8,
    pub(crate) is_column: bool,
}

impl AreaSeed {
    /// Packs the ranking as the run is named, so that the queue never
    /// has to look at anything but `order` to compare two runs.
    ///
    /// The bits, from the top: fifteen of length, one set when the run
    /// is a row, then the row and the column of its first cell, each
    /// stored as `255 - v` so that a smaller coordinate sorts higher.
    /// Reading the whole thing as one `u32` therefore orders by longest
    /// first, then a row run over a column run, then upper-left-most.
    pub(crate) fn new(line: u8, span: Span, is_column: bool) -> Self {
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

    /// How many positions the run covers.
    pub(crate) fn len(&self) -> u16 {
        self.end as u16 - self.start as u16 + 1
    }

    /// The run without the line it sits on.
    pub(crate) fn span(&self) -> Span {
        Span { start: self.start, end: self.end }
    }

    /// Whether the run this names is still standing, unchanged.
    pub(crate) fn standing(&self, rows: &Runs, cols: &Runs) -> bool {
        let side = if self.is_column { cols } else { rows };
        side.span_at(self.line, self.start) == Some(self.span())
    }

}

/// Covers a seed with every cell standing under it, in as few
/// rectangles as that takes.
///
/// Merging two neighbouring stretches can only lower the ceiling and
/// raise the floor of what they share, so a cover takes every cell under
/// the seed exactly when no stretch holds two crossing runs that differ.
/// That fixes where the cuts go, and the fewest rectangles managing it
/// is one per stretch of equal runs: no charge to tune, no search.
///
/// It meshes worse than taking the seed whole, deliberately. The
/// rectangles it leaves are thin, and thin rectangles are the ones a
/// rewriting pass can do something with.
pub(crate) fn take_all_area(
    crossing: &Runs,
    seed: Span,
    line: u8,
    seed_is_column: bool,
    out: &mut Vec<Rect>,
) {
    let emit = |out: &mut Vec<Rect>, from: u8, to: u8, run: Span| {
        out.push(if seed_is_column {
            Rect { x0: run.start, y0: from, x1: run.end, y1: to }
        } else {
            Rect { x0: from, y0: run.start, x1: to, y1: run.end }
        });
    };

    let mut open: Option<(u8, Span)> = None;
    for pos in seed.start..=seed.end {
        let run = crossing
            .span_at(pos, line)
            .expect("a cell still standing belongs to a run of either kind");
        match open {
            Some((from, current)) if current != run => {
                emit(out, from, pos - 1, current);
                open = Some((pos, run));
            }
            None => open = Some((pos, run)),
            _ => {}
        }
    }

    if let Some((from, current)) = open {
        emit(out, from, seed.end, current);
    }
}

/// How much area stands in the runs crossing a seed: the lengths of all
/// of them added up, not the longest of them.
fn crossing_area(seed: &AreaSeed, rows: &Runs, cols: &Runs) -> u32 {
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
/// No run sits in this slot.
const NO_SEED: u32 = u32::MAX;

/// The runs waiting to be seeded, bucketed by length.
///
/// The only thing the queue is ever asked for is every run at the
/// longest length left, and a length is 1 to 256, so the order is an
/// array index rather than a comparison. Carving a run leaves pieces
/// strictly shorter than it, and no run longer than the level's length
/// is standing to be carved, so nothing can ever land in a bucket the
/// cursor has already passed. The cursor only descends, every push is
/// two writes, and drawing a level is a walk down one chain.
///
/// The chains live in one arena that only grows, rather than in 257
/// vectors that would each have to find their own size. Run and link
/// are kept side by side rather than paired, because a pair of them is
/// twelve bytes and a twelve-byte push is a call to memcpy.
pub(crate) struct Queue {
    /// Every run pushed, in the order they were pushed.
    seeds: Vec<AreaSeed>,
    /// For each of those, the slot of the next run in its bucket.
    next: Vec<u32>,
    /// The run pushed most recently at each length.
    heads: Box<[u32; 257]>,
    longest: usize,
}

impl Queue {
    /// Enough room for the runs a realistic bitmap goes through without
    /// the arena having to find more.
    const EXPECTED: usize = 8192;

    /// An empty queue, with room already found for the runs a
    /// realistic bitmap will put through it.
    pub(crate) fn new() -> Self {
        Self {
            seeds: Vec::with_capacity(Self::EXPECTED),
            next: Vec::with_capacity(Self::EXPECTED),
            heads: Box::new([NO_SEED; 257]),
            longest: 256,
        }
    }

    /// Empties the queue, ready for another bitmap.
    pub(crate) fn reset(&mut self) {
        self.seeds.clear();
        self.next.clear();
        self.heads.fill(NO_SEED);
        self.longest = 256;
    }

    /// Files a run under its length: two writes and no comparison.
    ///
    /// Safe to do at any point in the mesh because a run's length never
    /// changes. Carving either takes a run away or leaves it alone, and
    /// what it leaves behind is a new run pushed in its own right, so a
    /// run sitting in a bucket is either exactly what it says it is or
    /// gone, and one lookup tells which.
    pub(crate) fn push(&mut self, seed: AreaSeed) {
        let length = seed.len() as usize;
        let slot = self.seeds.len() as u32;
        self.seeds.push(seed);
        self.next.push(self.heads[length]);
        self.heads[length] = slot;
    }

    /// Empties the bucket at the longest length that has anything in it
    /// into `out`, and answers that length.
    fn drain_longest(&mut self, out: &mut Vec<AreaSeed>) -> Option<u16> {
        loop {
            let head = self.heads[self.longest];
            if head != NO_SEED {
                self.heads[self.longest] = NO_SEED;
                let mut at = head;
                while at != NO_SEED {
                    out.push(self.seeds[at as usize]);
                    at = self.next[at as usize];
                }
                return Some(self.longest as u16);
            }
            if self.longest == 0 {
                return None;
            }
            self.longest -= 1;
        }
    }
}

/// The runs tied at the longest length left, ranked among themselves
/// by crossing area.
///
/// Held apart from the queue because the two are ordered by figures
/// that behave differently. Length never changes, so the queue can be
/// left stale. Crossing area falls as the bitmap is carved, and a
/// figure that improves cannot be left stale in an order that prefers
/// it small, because a run that got better would sit buried under runs
/// that had not. So the area is settled only among the runs actually
/// tied on length, which is the only place it was ever consulted, and
/// the ones a carve could have reached are recounted at once.
pub(crate) struct Level {
    /// The length every run in here is tied at.
    length: u16,
    /// Slot to run, and beside it the area in force for that run, or
    /// [`SPENT`]. Kept apart for the same reason the queue keeps its
    /// links apart from its runs.
    runs: Vec<AreaSeed>,
    areas: Vec<u32>,
    /// Slots by how good they look, best first. Rankings are pushed and
    /// never updated in place, and a superseded one is recognised on
    /// the way out by its area no longer matching the slot's.
    order: BinaryHeap<Ranked>,
    /// For each slot, the next slot in the same bucket. A run belongs
    /// to exactly one bucket, so the chains need no arena of their own:
    /// they run through the slots themselves, where 512 vectors used to
    /// be 512 heap blocks to find, fill and give back every bitmap.
    next: Vec<u32>,
    /// The slot most recently put in each bucket, by orientation and by
    /// where the run starts.
    heads: Box<[u32; 2 * Self::POSITIONS]>,
    /// Which start positions hold anything, by orientation. A carve
    /// dirties a range of starts as wide as the level's runs are long,
    /// and nearly all of them are empty, so the range is walked as set
    /// bits rather than position by position.
    occupied: [[u64; LINE_WORDS]; 2],
    /// The buckets holding anything, so that emptying the level walks
    /// those rather than all five hundred.
    filled: Vec<usize>,
    /// The bucket the queue last handed over, standing or not.
    drawn: Vec<AreaSeed>,
}

impl Level {
    /// How many positions a run can start at, which is how wide each
    /// orientation's half of the bucket array is.
    const POSITIONS: usize = 256;

    /// An empty level. One is built per workspace and reused.
    pub(crate) fn new() -> Self {
        Self {
            length: 0,
            runs: Vec::new(),
            areas: Vec::new(),
            next: Vec::new(),
            order: BinaryHeap::new(),
            heads: Box::new([NO_SEED; 2 * Self::POSITIONS]),
            occupied: [[0; LINE_WORDS]; 2],
            filled: Vec::new(),
            drawn: Vec::new(),
        }
    }

    /// Empties the level, ready for another bitmap.
    pub(crate) fn reset(&mut self) {
        for &bucket in &self.filled {
            self.heads[bucket] = NO_SEED;
        }
        self.filled.clear();
        self.occupied = [[0; LINE_WORDS]; 2];
        self.length = 0;
        self.runs.clear();
        self.areas.clear();
        self.next.clear();
        self.order.clear();
        self.drawn.clear();
    }

    /// Which bucket a run belongs to: its orientation and where it
    /// starts, which is everything [`Level::note`] needs to find the
    /// runs a carve could have reached.
    fn bucket(is_column: bool, start: u8) -> usize {
        usize::from(is_column) * Self::POSITIONS + start as usize
    }

    /// Ranks an area so the smaller one sorts higher. Seventeen bits
    /// hold an area, which cannot exceed the 65536 cells of the matrix.
    fn key(&self, seed: &AreaSeed, area: u32) -> u64 {
        ((0x1_FFFF - area as u64) << 26) | seed.order as u64
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
    fn bound(&self) -> u32 {
        self.length as u32
    }

    /// Draws every run standing at the longest length left.
    ///
    /// A bucket can come back holding nothing but runs that have since
    /// been carved away, which is no level at all, so the cursor keeps
    /// descending until one of them is still standing.
    pub(crate) fn draw(&mut self, queue: &mut Queue, rows: &Runs, cols: &Runs) {
        for &bucket in &self.filled {
            self.heads[bucket] = NO_SEED;
        }
        self.filled.clear();
        self.occupied = [[0; LINE_WORDS]; 2];
        self.runs.clear();
        self.areas.clear();
        self.next.clear();
        self.order.clear();

        loop {
            self.drawn.clear();
            let Some(length) = queue.drain_longest(&mut self.drawn) else { return };
            self.length = length;

            for index in 0..self.drawn.len() {
                let seed = self.drawn[index];
                if !seed.standing(rows, cols) {
                    continue;
                }

                let slot = self.runs.len() as u32;
                let key = self.key(&seed, self.bound());
                self.order.push(Ranked { key, area: UNCOUNTED, slot });
                self.runs.push(seed);
                self.areas.push(UNCOUNTED);

                let bucket = Self::bucket(seed.is_column, seed.start);
                if self.heads[bucket] == NO_SEED {
                    self.filled.push(bucket);
                    self.occupied[usize::from(seed.is_column)][seed.start as usize / 64] |=
                        1 << (seed.start % 64);
                }
                self.next.push(self.heads[bucket]);
                self.heads[bucket] = slot;
            }

            if !self.runs.is_empty() {
                return;
            }
        }
    }

    /// The best run left in the level, or `None` once it is exhausted.
    pub(crate) fn take_best(&mut self, rows: &Runs, cols: &Runs) -> Option<AreaSeed> {
        while let Some(top) = self.order.pop() {
            let slot = top.slot as usize;
            let (seed, area) = (self.runs[slot], self.areas[slot]);
            if top.area != area {
                continue;
            }
            if !seed.standing(rows, cols) {
                self.areas[slot] = SPENT;
                continue;
            }

            // Standing at its best possible figure, so settle it and let
            // it find its real place.
            if area == UNCOUNTED {
                let counted = crossing_area(&seed, rows, cols);
                self.areas[slot] = counted;
                self.order.push(Ranked {
                    key: self.key(&seed, counted),
                    area: counted,
                    slot: top.slot,
                });
                continue;
            }

            self.areas[slot] = SPENT;
            return Some(seed);
        }
        None
    }

    /// Recounts the runs the carve could have reached.
    ///
    /// A run's crossing area changes only if the carve reached a run
    /// crossing it, and the carve reached rows `rect.y0..=rect.y1` and
    /// columns `rect.x0..=rect.x1`. So the row runs to recount are the
    /// ones overlapping the carve's columns and the column runs are the
    /// ones overlapping its rows. A run of this level's length
    /// overlapping `lo..=hi` has to start somewhere in
    /// `lo - (length - 1) ..= hi`, so only those buckets are visited.
    pub(crate) fn note(&mut self, rect: &Rect, rows: &Runs, cols: &Runs) {
        for is_column in [false, true] {
            let (lo, hi) = if is_column { (rect.y0, rect.y1) } else { (rect.x0, rect.x1) };
            let first = (lo as i32 - self.length as i32 + 1).max(0) as u8;
            let last = hi;

            let occupied = self.occupied[usize::from(is_column)];
            for (index, word) in occupied.iter().enumerate() {
                let mut starts = word & range_mask(index, first, last);
                while starts != 0 {
                    let start = (index * 64 + starts.trailing_zeros() as usize) as u8;
                    starts &= starts - 1;

                    let mut at = self.heads[Self::bucket(is_column, start)];
                    while at != NO_SEED {
                        let slot = at as usize;
                        at = self.next[slot];

                        let (seed, area) = (self.runs[slot], self.areas[slot]);
                        // Taken already, or still standing at a figure
                        // that cannot be beaten by the area falling
                        // further.
                        if area == SPENT || area == UNCOUNTED {
                            continue;
                        }
                        // The carve may have taken it away rather than
                        // merely reached it, and there is nothing left
                        // to measure then.
                        if !seed.standing(rows, cols) {
                            self.areas[slot] = SPENT;
                            continue;
                        }
                        let now = crossing_area(&seed, rows, cols);
                        if now != area {
                            self.areas[slot] = now;
                            self.order.push(Ranked {
                                key: self.key(&seed, now),
                                area: now,
                                slot: slot as u32,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// The same mesh, worked out by scanning every run each step instead of
/// keeping a queue. Slow, obviously right, and what the fast path is
/// checked against.
#[doc(hidden)]
pub fn mesh_by_scanning(source: &BitMatrix) -> Vec<Rect> {
    let (alone, source) = source.split_isolated();
    let source = &source;

    let (mut rows, mut cols) = Runs::of(source);
    let mut rects = Vec::new();
    let (mut plan, mut bin) = (Vec::new(), Vec::new());

    while let Some(seed) = scan_for_area_seed(&rows, &cols) {
        let crossing = if seed.is_column { &rows } else { &cols };
        plan.clear();
        take_all_area(crossing, seed.span(), seed.line, seed.is_column, &mut plan);

        for rect in plan.drain(..) {
            rows.carve((rect.y0, rect.y1), rect.x0, rect.x1, &mut bin);
            cols.carve((rect.x0, rect.x1), rect.y0, rect.y1, &mut bin);
            bin.clear();
            rects.push(rect);
        }
    }

    alone.for_each_set(|x, y| rects.push(Rect { x0: x, y0: y, x1: x, y1: y }));
    rects
}

/// The longest run left, the one with the least crossing area among
/// those tied on length, and the upper-left-most among those tied on
/// both.
fn scan_for_area_seed(rows: &Runs, cols: &Runs) -> Option<AreaSeed> {
    let mut longest = 0;
    for side in [rows, cols] {
        side.for_each_run(|_, span| longest = longest.max(span.len()));
    }
    if longest == 0 {
        return None;
    }

    let mut best: Option<(AreaSeed, u32)> = None;
    for (is_column, side) in [(false, rows), (true, cols)] {
        side.for_each_run(|line, span| {
            if span.len() != longest {
                return;
            }
            let seed = AreaSeed::new(line, span, is_column);
            let area = crossing_area(&seed, rows, cols);
            let better = match best {
                None => true,
                Some((_, top_area)) if area != top_area => area < top_area,
                Some((top, _)) => seed.order > top.order,
            };
            if better {
                best = Some((seed, area));
            }
        });
    }

    best.map(|(seed, _)| seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The straightforward way to find runs: look at every cell. Kept
    /// as the reference the fast one is checked against.
    fn runs_cell_by_cell(set: impl Fn(u8, u8) -> bool) -> Vec<(u8, Span)> {
        let mut found = Vec::new();
        for line in 0..=u8::MAX {
            let mut start: Option<u8> = None;
            for pos in 0..=u8::MAX {
                match (set(line, pos), start) {
                    (true, None) => start = Some(pos),
                    (false, Some(s)) => {
                        found.push((line, Span { start: s, end: pos - 1 }));
                        start = None;
                    }
                    _ => {}
                }
            }
            if let Some(s) = start {
                found.push((line, Span { start: s, end: u8::MAX }));
            }
        }
        found
    }

    fn listed(runs: &Runs) -> Vec<(u8, Span)> {
        let mut found = Vec::new();
        runs.for_each_run(|line, span| found.push((line, span)));
        found
    }

    fn assert_same_runs(bits: &BitMatrix) {
        let (rows, cols) = Runs::of(bits);
        let want_rows = runs_cell_by_cell(|line, pos| bits.get(pos, line));
        let want_cols = runs_cell_by_cell(|line, pos| bits.get(line, pos));
        assert_eq!(listed(&rows), want_rows, "row runs differ");
        assert_eq!(listed(&cols), want_cols, "column runs differ");

        // Every standing cell has to read back the run it belongs to,
        // and every cleared one has to read back nothing.
        for (line, span) in &want_rows {
            for pos in span.start..=span.end {
                assert_eq!(rows.span_at(*line, pos), Some(*span), "row {line} pos {pos}");
            }
        }
        for line in 0..=u8::MAX {
            for pos in 0..=u8::MAX {
                if !bits.get(pos, line) {
                    assert_eq!(rows.span_at(line, pos), None, "row {line} pos {pos}");
                }
            }
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

        // And content of every shape, which is where runs of every
        // length and every alignment turn up together.
        for (density, cluster) in
            [(0.02, 0.0), (0.02, 0.9), (0.2, 0.0), (0.2, 0.7), (0.5, 0.7), (0.9, 0.9)]
        {
            for bits in crate::samples::grown(0, density, cluster, 2) {
                assert_same_runs(&bits);
            }
        }
    }
}
