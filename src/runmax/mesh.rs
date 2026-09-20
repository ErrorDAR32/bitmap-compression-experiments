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

use crate::data::{Runs, Span};
use crate::{BitMatrix, Rect};

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
/// that -- it falls as the bitmap is carved -- so a seed sitting in the
/// queue on a figure counted long ago is sitting on a figure it may no
/// longer earn, and the queue has no way to tell. Keeping the area out
/// of it and settling that only among the runs actually tied on length
/// sidesteps the question, and costs nothing: the tie is the only place
/// the area was ever consulted.
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
/// left stale: a seed in it either names a run that is still exactly
/// what it says, or names one that is gone, and one lookup tells which.
/// Crossing area is not like that -- a carve makes it fall. So the area
/// is settled only among the runs actually tied on length, which is the
/// only place it was ever consulted.
///
/// Within a level the area is measured once, when the level is drawn,
/// and the order it gives is not revisited as carves eat into the runs
/// still waiting. That is a decision and not an oversight. The level
/// used to recount every run a carve could have reached and push it
/// again, which is why the runs were bucketed by where they started and
/// why a superseded ranking had to be recognised on the way out. It
/// bought nothing: over 540 generated bitmaps the fresh figures were
/// worth ten rectangles in three million, 0.0003%, and the machinery
/// that kept them fresh cost 16.6% of the whole run. Measuring once and
/// living with it is the better trade, and it leaves the level a list
/// with a cursor rather than a heap with an arena beside it.
pub(crate) struct Level {
    /// The runs of this level with their ranking keys, best first.
    ///
    /// A sorted list rather than a heap, because a run is ranked once
    /// and never reranked: one sort of a few dozen entries beats a push
    /// and a pop apiece, and the cursor below is the whole of what
    /// popping used to mean.
    ranked: Vec<(u64, AreaSeed)>,
    /// How far through `ranked` [`Level::take_best`] has got.
    at: usize,
    /// The bucket the queue last handed over, standing or not.
    drawn: Vec<AreaSeed>,
}

impl Level {
    /// An empty level. One is built per workspace and reused.
    pub(crate) fn new() -> Self {
        Self { ranked: Vec::new(), at: 0, drawn: Vec::new() }
    }

    /// Empties the level, ready for another bitmap.
    pub(crate) fn reset(&mut self) {
        self.ranked.clear();
        self.at = 0;
        self.drawn.clear();
    }

    /// Ranks a run so that the better one sorts higher: the larger
    /// crossing area, then the earlier position. Seventeen bits hold an
    /// area, which cannot exceed the 65536 cells of the matrix, and the
    /// twenty-six below it hold the seed's own ranking.
    ///
    /// Larger area, not smaller, and that was worth 8.8% of every
    /// rectangle the algorithm spends over the minimum. The old reading
    /// was that a run crossed by little does the least damage when it
    /// is taken, so take it first. What the corpus says is the
    /// opposite: a run crossed by a lot stands in thick content, where
    /// whatever it leaves behind is still wide enough to be covered
    /// cheaply, while a run crossed by little is in a thin place where
    /// the cells it strands have nowhere to go. Taking the thin ones
    /// last means taking them once the thick ones have already claimed
    /// what would have stranded them.
    ///
    /// Measured over 540 generated bitmaps, nine shapes by sixty
    /// seeds: 3.553% over the minimum before, 3.239% after.
    ///
    /// No two runs can share a key, since `order` names a run's
    /// orientation and where it starts, so the sort has nothing to
    /// break a tie on and the result does not depend on how it sorts.
    fn key(seed: &AreaSeed, area: u32) -> u64 {
        ((area as u64) << 26) | seed.order as u64
    }

    /// Draws every run standing at the longest length left, counts what
    /// crosses each one, and puts them in the order they will be taken
    /// in.
    ///
    /// A bucket can come back holding nothing but runs that have since
    /// been carved away, which is no level at all, so the cursor keeps
    /// descending until one of them is still standing.
    pub(crate) fn draw(&mut self, queue: &mut Queue, rows: &Runs, cols: &Runs) {
        self.ranked.clear();
        self.at = 0;

        loop {
            self.drawn.clear();
            let Some(_length) = queue.drain_longest(&mut self.drawn) else { return };

            for index in 0..self.drawn.len() {
                let seed = self.drawn[index];
                if !seed.standing(rows, cols) {
                    continue;
                }
                let area = crossing_area(&seed, rows, cols);
                self.ranked.push((Self::key(&seed, area), seed));
            }

            if !self.ranked.is_empty() {
                // Best first. Unstable is safe and is the cheaper sort:
                // the keys are all different, so there is no order
                // between equals for it to disturb.
                self.ranked.sort_unstable_by(|a, b| b.0.cmp(&a.0));
                return;
            }
        }
    }

    /// The best run left in the level, or `None` once it is exhausted.
    ///
    /// A run ranked when the level was drawn may have been carved away
    /// since by a run taken ahead of it, so what the cursor hands over
    /// is checked against the bitmap before it is answered.
    pub(crate) fn take_best(&mut self, rows: &Runs, cols: &Runs) -> Option<AreaSeed> {
        while self.at < self.ranked.len() {
            let seed = self.ranked[self.at].1;
            self.at += 1;
            if seed.standing(rows, cols) {
                return Some(seed);
            }
        }
        None
    }
}


/// The same mesh, worked out by scanning every run each step instead of
/// keeping a queue. Slow, obviously right, and what the fast path is
/// checked against.
///
/// It has to model the level, because the level is part of what the
/// mesh means and not merely how it is computed: the crossing areas
/// that order a level are the ones in force when the level was drawn,
/// not the ones in force when each run is reached. Taking the freshest
/// area each step -- which is what this used to do, and what it would
/// do if it just scanned for the best run every time -- is a different
/// and very slightly better algorithm that costs a sixth of the run to
/// implement. What stays independent is everything else: this keeps no
/// queue, no cursor and no bitmask, scans every run from the bitmap
/// each time it draws, and shares nothing with the fast path but the
/// rule it is spelling out.
#[doc(hidden)]
pub fn mesh_by_scanning(source: &BitMatrix) -> Vec<Rect> {
    let (alone, source) = source.split_isolated();
    let source = &source;

    let (mut rows, mut cols) = Runs::of(source);
    let mut rects = Vec::new();
    let (mut plan, mut bin) = (Vec::new(), Vec::new());
    let mut level = Vec::new();

    while scan_for_level(&rows, &cols, &mut level) {
        for seed in level.drain(..) {
            // Something taken ahead of it may have carved it away.
            if !seed.standing(&rows, &cols) {
                continue;
            }

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
    }

    alone.for_each_set(|x, y| rects.push(Rect { x0: x, y0: y, x1: x, y1: y }));
    rects
}

/// Every run standing at the longest length left, in the order they are
/// to be taken in: the greatest crossing area first, and among those
/// tied on that, the upper-left-most. Answers whether it found any.
///
/// A carve only ever shortens a run, so once a level has been worked
/// through no run of that length can still be standing and the next
/// scan is bound to find a shorter one. That is what makes drawing a
/// level at a time the same thing as scanning for the longest run every
/// step, apart from the areas being older.
fn scan_for_level(rows: &Runs, cols: &Runs, level: &mut Vec<AreaSeed>) -> bool {
    let mut longest = 0;
    for side in [rows, cols] {
        side.for_each_run(|_, span| longest = longest.max(span.len()));
    }
    if longest == 0 {
        return false;
    }

    let mut ranked: Vec<(u32, AreaSeed)> = Vec::new();
    for (is_column, side) in [(false, rows), (true, cols)] {
        side.for_each_run(|line, span| {
            if span.len() != longest {
                return;
            }
            let seed = AreaSeed::new(line, span, is_column);
            ranked.push((crossing_area(&seed, rows, cols), seed));
        });
    }

    // Biggest area first, then the seed's own ranking, which is what
    // [`Level::key`] packs into one number and sorts on there.
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.order.cmp(&a.1.order)));
    level.clear();
    level.extend(ranked.into_iter().map(|(_, seed)| seed));
    true
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
