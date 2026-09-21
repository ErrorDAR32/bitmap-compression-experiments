//! Runmax: the mesh, and everything it is built out of.
//!
//! The bitmap is reduced to the cells still standing in both
//! orientations, and every step afterwards works on those rather than
//! on cells. Each step takes the longest run still standing, in either
//! orientation, as one area a cell thick, and carves it out of both
//! sides. Whatever stood under it is still standing and will be some
//! later step's seed.
//!
//! Thin on purpose. The areas this leaves are the ones
//! [`crate::runmax::grow`] and [`crate::runmax::merge`] can do
//! something with, and it leaves a great many of them: 5531 on a
//! middling ragged bitmap, which the rewriting pass takes down to 5209.
//! Merging alone reclaims 259.4 areas a bitmap here, against 86.8 when
//! the mesh took more than the run.
//!
//! It used to take the run *and* every cell standing under it, which is
//! in [`take_all_area`] along with the witness that settled it the
//! other way.
//!
//! Three structures carry a step. [`Runs`] holds what is standing, as
//! bits. [`Queue`] holds the runs waiting to be seeded, bucketed by
//! length. [`Level`] holds the runs tied at the longest length left,
//! which is the only place the crossing area is ever consulted. Each
//! one is there because a plainer version of it was measured and cost
//! too much; the measurements are in their own docs.

use crate::chords::{Chords, CORNERS, CORNER_WORDS};
use crate::data::bits::range_mask;
use crate::data::{bounds, List, Run, Runs};
use crate::{BitMatrix, Area};

/// A run waiting to be seeded: the run itself, in whichever
/// orientation it lies, plus the ranking the queue sorts it by.
///
/// Named for all three of those, because "seed" on its own says
/// nothing -- the crate also seeds a random generator with one and
/// starts a merge from a list of them, and none of the three is the
/// others. This is the one that is a run.
///
/// The ranking packed into `order` is the seed run's length, then a row run
/// over a column run, then the upper-left-most, so the queue compares
/// seed runs with a single instruction rather than walking a chain of
/// fields. The crossing area is deliberately not in it.
///
/// Leaving it out is what makes the queue safe to leave stale. A seed run's
/// length never changes: carving either takes a seed run away or leaves it
/// alone, and what it leaves behind is a new seed run, queued in its own
/// right. So a seed run sitting in the queue is either exactly what it says
/// it is or gone, and one lookup tells which. Crossing area is not like
/// that -- it falls as the bitmap is carved -- so a seed run sitting in the
/// queue on a figure counted long ago is sitting on a figure it may no
/// longer earn, and the queue has no way to tell. Keeping the area out
/// of it and settling that only among the runs actually tied on length
/// sidesteps the question, and costs nothing: the tie is the only place
/// the area was ever consulted.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct AreaRunSeed {
    order: u32,
    pub(crate) line: u8,
    start: u8,
    end: u8,
    pub(crate) is_column: bool,
}

impl AreaRunSeed {
    /// Packs the ranking as the seed run is named, so that the queue never
    /// has to look at anything but `order` to compare two runs.
    ///
    /// The bits, from the top: fifteen of length, one set when the seed run
    /// is a row, then the row and the column of its first cell, each
    /// stored as `255 - v` so that a smaller coordinate sorts higher.
    /// Reading the whole thing as one `u32` therefore orders by longest
    /// first, then a row run over a column run, then upper-left-most.
    pub(crate) fn new(line: u8, span: Run, is_column: bool) -> Self {
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

    /// How many positions the seed run covers.
    pub(crate) fn len(&self) -> u16 {
        self.end as u16 - self.start as u16 + 1
    }

    /// The seed run without the line it sits on.
    pub(crate) fn span(&self) -> Run {
        Run { start: self.start, end: self.end }
    }

    /// Whether the seed run this names is still standing, unchanged.
    pub(crate) fn standing(&self, rows: &Runs, cols: &Runs) -> bool {
        let side = if self.is_column { cols } else { rows };
        side.run_at(self.line, self.start) == Some(self.span())
    }

}

/// Takes the seed run and nothing else: one area, one cell thick.
///
/// This used to cover every cell standing under the seed run as well --
/// one area per stretch of agreeing crossing runs, turning one seed
/// into up to 256 areas. The argument for it was that covering leaves
/// thin areas, and thin areas are what the rewriting pass can do
/// something with.
///
/// The argument is backwards, and it took a minimal witness to see it.
/// Shrinking the bitmaps runmax is furthest over the minimum on turns
/// up shapes like this one, where the mesh spends four areas on a shape
/// worth three:
///
/// ```text
///     .##.        .bc.        .aa.
///     ####   ->   abcd   vs   bbbb
///     .#..        .b..        .c..
///                 covered     fewest
/// ```
///
/// The mesh seeds on the row of four, which is right -- and then covers
/// under it, which cuts that row into four areas because the columns
/// below it disagree. Taking the run whole leaves the row, a domino
/// above it and a single cell, which is the minimum.
///
/// An area one cell thick is thinner than anything covering leaves, so
/// the rewriting pass gets more to work with rather than less: merging
/// goes from reclaiming 86.8 areas a bitmap to 259.4. Over 540 bitmaps
/// the partition lands 1.925% over the minimum where covering landed
/// 3.213%, for 1.7% more instructions per active cell.
///
/// The figures that settled it the other way -- 84.27 areas a bitmap
/// against 76.06, 74.66 against 75.19 after rewriting -- were taken on
/// a hand-drawn corpus that no longer exists, against a mesher that
/// went with it, and were never retaken when the generated corpus
/// replaced it.
pub(crate) fn take_all_area(
    crossing: &Runs,
    run: Run,
    line: u8,
    run_is_column: bool,
    out: &mut List<Area, { bounds::PLAN }>,
) {
    // The crossing runs settle nothing now: what is taken is the seed
    // run itself, and what stands under it is left for the next seed.
    let _ = crossing;
    out.push(if run_is_column {
        Area { x0: line, y0: run.start, x1: line, y1: run.end }
    } else {
        Area { x0: run.start, y0: line, x1: run.end, y1: line }
    });
}

/// How much area stands in the runs crossing a seed run: the lengths of all
/// of them added up, not the longest of them.
fn crossing_area(seed_run: &AreaRunSeed, rows: &Runs, cols: &Runs) -> u32 {
    let crossing = if seed_run.is_column { rows } else { cols };
    let mut crossing_cells = 0;
    for pos in seed_run.start..=seed_run.end {
        if let Some(across) = crossing.run_at(pos, seed_run.line) {
            crossing_cells += across.len() as u32;
        }
    }
    crossing_cells
}

/// No seed run sits in this slot.
const NO_SEED: u32 = u32::MAX;

/// The runs waiting to be seeded, bucketed by length.
///
/// The only thing the queue is ever asked for is every seed run at the
/// longest length left, and a length is 1 to 256, so the order is an
/// array index rather than a comparison. Carving a seed run leaves pieces
/// strictly shorter than it, and no seed run longer than the level's length
/// is standing to be carved, so nothing can ever land in a bucket the
/// cursor has already passed. The cursor only descends, every push is
/// two writes, and drawing a level is a walk down one chain.
///
/// The chains live in one arena that only grows, rather than in 257
/// vectors that would each have to find their own size. Run and link
/// are kept side by side rather than paired, because a pair of them is
/// twelve bytes and a twelve-byte push is a call to memcpy.
pub(crate) struct Queue {
    /// Every seed run pushed, in the order they were pushed.
    runs: List<AreaRunSeed, { bounds::QUEUE }>,
    /// For each of those, the slot of the next seed run in its bucket.
    next: List<u32, { bounds::QUEUE }>,
    /// The seed run pushed most recently at each length.
    heads: Box<[u32; 257]>,
    longest: usize,
}

impl Queue {

    /// An empty queue, with room already found for the runs a
    /// realistic bitmap will put through it.
    pub(crate) fn new() -> Self {
        Self {
            runs: List::new(),
            next: List::new(),
            heads: Box::new([NO_SEED; 257]),
            longest: 256,
        }
    }

    /// Empties the queue, ready for another bitmap.
    pub(crate) fn reset(&mut self) {
        self.runs.clear();
        self.next.clear();
        self.heads.fill(NO_SEED);
        self.longest = 256;
    }

    /// Files a seed run under its length: two writes and no comparison.
    ///
    /// Safe to do at any point in the mesh because a seed run's length never
    /// changes. Carving either takes a seed run away or leaves it alone, and
    /// what it leaves behind is a new seed run pushed in its own right, so a
    /// seed run sitting in a bucket is either exactly what it says it is or
    /// gone, and one lookup tells which.
    pub(crate) fn push(&mut self, seed_run: AreaRunSeed) {
        let length = seed_run.len() as usize;
        let slot = self.runs.len() as u32;
        self.runs.push(seed_run);
        self.next.push(self.heads[length]);
        self.heads[length] = slot;
    }

    /// Empties the bucket at the longest length that has anything in it
    /// into `out`, and answers that length.
    fn drain_longest(&mut self, out: &mut List<AreaRunSeed, { bounds::LEVEL }>) -> Option<u16> {
        loop {
            let head = self.heads[self.longest];
            if head != NO_SEED {
                self.heads[self.longest] = NO_SEED;
                let mut at = head;
                while at != NO_SEED {
                    out.push(self.runs[at as usize]);
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
/// left stale: a seed run in it either names a seed run that is still exactly
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
/// that kept them fresh cost 16.6% of the whole seed run. Measuring once and
/// living with it is the better trade, and it leaves the level a list
/// with a cursor rather than a heap with an arena beside it.
pub(crate) struct Level {
    /// The runs of this level with their ranking keys, best first.
    ///
    /// A sorted list rather than a heap, because a seed run is ranked once
    /// and never reranked: one sort of a few dozen entries beats a push
    /// and a pop apiece, and the cursor below is the whole of what
    /// popping used to mean.
    ranked: List<(u64, AreaRunSeed), { bounds::LEVEL }>,
    /// How far through `ranked` [`Level::take_best`] has got.
    at: usize,
    /// The bucket the queue last handed over, standing or not.
    drawn: List<AreaRunSeed, { bounds::LEVEL }>,
}

impl Level {
    /// An empty level. One is built per workspace and reused.
    pub(crate) fn new() -> Self {
        Self { ranked: List::new(), at: 0, drawn: List::new() }
    }

    /// Empties the level, ready for another bitmap.
    pub(crate) fn reset(&mut self) {
        self.ranked.clear();
        self.at = 0;
        self.drawn.clear();
    }

    /// Ranks a seed run so that the better one sorts higher: the larger
    /// crossing area, then the earlier position. Seventeen bits hold an
    /// area, which cannot exceed the 65536 cells of the matrix, and the
    /// twenty-six below it hold the seed run's own ranking.
    ///
    /// Larger area, not smaller, and that was worth 8.8% of every
    /// rectangle the algorithm spends over the minimum. The old reading
    /// was that a seed run crossed by little does the least damage when it
    /// is taken, so take it first. What the corpus says is the
    /// opposite: a seed run crossed by a lot stands in thick content, where
    /// whatever it leaves behind is still wide enough to be covered
    /// cheaply, while a seed run crossed by little is in a thin place where
    /// the cells it strands have nowhere to go. Taking the thin ones
    /// last means taking them once the thick ones have already claimed
    /// what would have stranded them.
    ///
    /// Measured over 540 generated bitmaps, nine shapes by sixty
    /// seeds: 3.553% over the minimum before, 3.239% after.
    ///
    /// No two runs can share a key, since `order` names a seed run's
    /// orientation and where it starts, so the sort has nothing to
    /// break a tie on and the result does not depend on how it sorts.
    fn key(seed_run: &AreaRunSeed, area: u32) -> u64 {
        ((area as u64) << 26) | seed_run.order as u64
    }

    /// Draws every seed run standing at the longest length left, counts what
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
                let seed_run = self.drawn[index];
                if !seed_run.standing(rows, cols) {
                    continue;
                }
                let area = crossing_area(&seed_run, rows, cols);
                self.ranked.push((Self::key(&seed_run, area), seed_run));
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

    /// The best seed run left in the level, or `None` once it is exhausted.
    ///
    /// A seed run ranked when the level was drawn may have been carved away
    /// since by a seed run taken ahead of it, so what the cursor hands over
    /// is checked against the bitmap before it is answered.
    pub(crate) fn take_best(&mut self, rows: &Runs, cols: &Runs) -> Option<AreaRunSeed> {
        while self.at < self.ranked.len() {
            let seed_run = self.ranked[self.at].1;
            self.at += 1;
            if seed_run.standing(rows, cols) {
                return Some(seed_run);
            }
        }
        None
    }
}


/// The same mesh, worked out by scanning every run each step instead of
/// keeping a queue. Slow, obviously right, and what the fast path is
/// checked against.
///
/// Allocates freely, unlike everything it checks: the fast path keeps
/// every list in a workspace found once, and this builds what it needs
/// each time it needs it. That is the right trade for a reference --
/// the one thing it must not share with the code it checks is the
/// cleverness.
///
/// It has to model the level, because the level is part of what the
/// mesh means and not merely how it is computed: the crossing areas
/// that order a level are the ones in force when the level was drawn,
/// not the ones in force when each seed run is reached. Taking the freshest
/// area each step -- which is what this used to do, and what it would
/// do if it just scanned for the best seed run every time -- is a different
/// and very slightly better algorithm that costs a sixth of the seed run to
/// implement. What stays independent is everything else: this keeps no
/// queue, no cursor and no bitmask, scans every run from the bitmap
/// each time it draws, and shares nothing with the fast path but the
/// rule it is spelling out.
#[doc(hidden)]
pub fn mesh_by_scanning(source: &BitMatrix) -> Vec<Area> {
    let (single_cells, source) = source.split_isolated();
    let source = &source;

    let (mut rows, mut cols) = Runs::of(source);
    // The chords are of the bitmap, not of what is left standing, so
    // they are found once before anything is carved.
    let mut corners = Corners::blank();
    corners.rebuild(source, &rows, &cols);
    let mut areas = Vec::new();
    let mut plan: List<Area, { bounds::PLAN }> = List::new();
    let mut bin: List<(u8, Run), { bounds::CUT }> = List::new();
    let mut level = Vec::new();

    while scan_for_level(&rows, &cols, &mut level) {
        for seed_run in level.drain(..) {
            // Something taken ahead of it may have carved it away.
            if !seed_run.standing(&rows, &cols) {
                continue;
            }

            let crossing = if seed_run.is_column { &rows } else { &cols };
            let span = corners.trim(&seed_run);
            plan.clear();
            take_all_area(crossing, span, seed_run.line, seed_run.is_column, &mut plan);

            for index in 0..plan.len() {
                let area = plan[index];
                rows.carve((area.y0, area.y1), area.x0, area.x1, &mut bin);
                cols.carve((area.x0, area.x1), area.y0, area.y1, &mut bin);
                bin.clear();
                areas.push(area);
            }
        }
    }

    single_cells.for_each_set(|x, y| areas.push(Area { x0: x, y0: y, x1: x, y1: y }));
    areas
}

/// Every seed run standing at the longest length left, in the order they are
/// to be taken in: the greatest crossing area first, and among those
/// tied on that, the upper-left-most. Answers whether it found any.
///
/// A carve only ever shortens a seed run, so once a level has been worked
/// through no seed run of that length can still be standing and the next
/// scan is bound to find a shorter one. That is what makes drawing a
/// level at a time the same thing as scanning for the longest seed run every
/// step, apart from the areas being older.
fn scan_for_level(rows: &Runs, cols: &Runs, level: &mut Vec<AreaRunSeed>) -> bool {
    let mut longest = 0;
    for side in [rows, cols] {
        side.for_each_run(|_, span| longest = longest.max(span.len()));
    }
    if longest == 0 {
        return false;
    }

    let mut ranked: Vec<(u32, AreaRunSeed)> = Vec::new();
    for (is_column, side) in [(false, rows), (true, cols)] {
        side.for_each_run(|line, span| {
            if span.len() != longest {
                return;
            }
            let seed_run = AreaRunSeed::new(line, span, is_column);
            ranked.push((crossing_area(&seed_run, rows, cols), seed_run));
        });
    }

    // Biggest area first, then the seed run's own ranking, which is what
    // [`Level::key`] packs into one number and sorts on there.
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.order.cmp(&a.1.order)));
    level.clear();
    level.extend(ranked.into_iter().map(|(_, seed_run)| seed_run));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The straightforward way to find runs: look at every cell. Kept
    /// as the reference the fast one is checked against.
    fn runs_cell_by_cell(set: impl Fn(u8, u8) -> bool) -> Vec<(u8, Run)> {
        let mut found = Vec::new();
        for line in 0..=u8::MAX {
            let mut start: Option<u8> = None;
            for pos in 0..=u8::MAX {
                match (set(line, pos), start) {
                    (true, None) => start = Some(pos),
                    (false, Some(s)) => {
                        found.push((line, Run { start: s, end: pos - 1 }));
                        start = None;
                    }
                    _ => {}
                }
            }
            if let Some(s) = start {
                found.push((line, Run { start: s, end: u8::MAX }));
            }
        }
        found
    }

    fn listed(runs: &Runs) -> Vec<(u8, Run)> {
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

        // Every standing cell has to read back the seed run it belongs to,
        // and every cleared one has to read back nothing.
        for (line, span) in &want_rows {
            for pos in span.start..=span.end {
                assert_eq!(rows.run_at(*line, pos), Some(*span), "row {line} pos {pos}");
            }
        }
        for line in 0..=u8::MAX {
            for pos in 0..=u8::MAX {
                if !bits.get(pos, line) {
                    assert_eq!(rows.run_at(line, pos), None, "row {line} pos {pos}");
                }
            }
        }
    }

    /// The word-wise pass has to agree with reading every cell, on
    /// everything from an empty bitmap to a full one, including runs
    /// that end exactly on a word boundary and ones that seed run to 255.
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

// ---------------------------------------------------------------------
// Where the partition is forced to cut, and what a seed run may cross.
// ---------------------------------------------------------------------

/// Where a drawn chord crosses a line of cells, which is where a seed
/// run has to stop.
///
/// The chords, and which of them are worth drawing, are
/// [`crate::chords`]'s to decide, and it decides them the same way for
/// both algorithms in the crate: a maximum independent set, by a
/// matching and Koenig's theorem. Stopping a seed run anywhere else
/// costs an area and buys nothing. Two weaker rules were tried and
/// both lose, at 6486 areas over the minimum before any of them:
///
/// | rule | over the minimum | instructions |
/// | --- | --- | --- |
/// | every reflex corner | 5464 | 331.3M |
/// | every chord | 3060 | 264.8M |
/// | chords that cross nothing | 3294 | 253.1M |
/// | a greedy independent set | 3143 | 253.4M |
///
/// Corners lose badly, because a lone corner's cut can be served in
/// either direction. Every chord loses for the same reason one step
/// up: only one of a meeting pair is ever worth drawing, so stopping
/// at the other spends an area for nothing.
///
/// Telling the two directions of a corner apart was tried and is a
/// tautology: a corner strictly inside a seed run has the run's own two
/// cells filled, so the cell it is missing is always on the far side
/// and its cut always runs across the run. Bit for bit the same
/// partition.
pub(crate) struct Corners {
    chords: Chords,
    /// For each row of cells, the lattice columns a drawn vertical
    /// chord covers there. A row seed run on line `l` is trimmed by
    /// `crossing_row[l]`.
    crossing_row: Box<[u64; CORNERS * CORNER_WORDS]>,
    /// The same for a column seed run, kept transposed so that it reads
    /// along a line rather than down a stride.
    crossing_col: Box<[u64; CORNERS * CORNER_WORDS]>,
    /// The same chords the other way round, a line of them at a time
    /// rather than a cell at a time: `across[l]` is the cells of
    /// lattice row `l` a horizontal chord lies along, and `down[l]` the
    /// cells of lattice column `l` a vertical chord lies along.
    ///
    /// The mesh wants a seed run's crossings, which is why the first
    /// pair is indexed by cell; growing wants to ask whether a whole
    /// lattice line is barred over a span, which is this one.
    across: Box<[u64; CORNERS * CORNER_WORDS]>,
    down: Box<[u64; CORNERS * CORNER_WORDS]>,
}

impl Corners {
    /// No chords anywhere.
    pub(crate) fn blank() -> Self {
        Self {
            chords: Chords::default(),
            crossing_row: Box::new([0; CORNERS * CORNER_WORDS]),
            crossing_col: Box::new([0; CORNERS * CORNER_WORDS]),
            across: Box::new([0; CORNERS * CORNER_WORDS]),
            down: Box::new([0; CORNERS * CORNER_WORDS]),
        }
    }

    /// Settles the chords and paints the lines the drawn ones cross.
    ///
    /// The runs are the ones the mesh has just built and has not carved
    /// yet, so the rows are the bitmap and the columns its transpose,
    /// which is what a chord scan needs read both ways.
    pub(crate) fn rebuild(&mut self, bits: &BitMatrix, rows: &Runs, cols: &Runs) {
        self.chords.rebuild(bits, rows, cols);
        self.crossing_row.fill(0);
        self.crossing_col.fill(0);
        self.across.fill(0);
        self.down.fill(0);

        for (chord, across) in self.chords.drawn() {
            // A horizontal chord on lattice row `line` crosses every
            // column of cells it spans, and a vertical one on lattice
            // column `line` every row, painted into the line the seed
            // run it crosses will read.
            let painted =
                if across { &mut self.crossing_col } else { &mut self.crossing_row };
            let line = chord.line as usize;
            for cell in chord.from as usize..chord.to as usize {
                painted[cell * CORNER_WORDS + line / 64] |= 1 << (line % 64);
            }

            // And the same chord along its own line, for growing.
            let along = if across { &mut self.across } else { &mut self.down };
            let along = &mut along[line * CORNER_WORDS..(line + 1) * CORNER_WORDS];
            for word in 0..CORNER_WORDS {
                along[word] |= range_mask(word, chord.from as u8, chord.to as u8 - 1);
            }
        }
    }

    /// Whether a chord bars a lattice line over a span of cells.
    ///
    /// Growing a rectangle across a chord destroys it: the chord ends
    /// up inside a face rather than between two, so it stops being a
    /// cut and the two corners it served are back to needing one each.
    /// `across` asks about a lattice row, which a rectangle growing up
    /// or down crosses, and its span is in columns.
    pub(crate) fn bars(&self, across: bool, line: u8, from: u8, to: u8) -> bool {
        let along = if across { &self.across } else { &self.down };
        let along = &along[line as usize * CORNER_WORDS..];
        (from as usize / 64..=to as usize / 64)
            .any(|word| along[word] & range_mask(word, from, to) != 0)
    }

    /// The seed run, cut back to the longest stretch of it that no
    /// drawn chord crosses. Answers the whole run when nothing crosses
    /// it.
    ///
    /// Stopping the area at a chord leaves the rest of the run
    /// standing, to be seeded in its own right, and puts the area's end
    /// on a cut the partition owed anyway.
    ///
    /// Keeping only the longest piece is what makes it pay. Emitting
    /// every piece of the run at once instead -- one carve rather than
    /// several, and nothing put back in the queue -- was measured and
    /// lost on both counts: 4843 areas over the minimum against 3294,
    /// and more instructions rather than fewer. The pieces have to go
    /// back in the queue and compete on length with everything else,
    /// because longest-first is the whole of what the mesh knows.
    ///
    /// The walk is bit at a time, which looks like the wrong shape for
    /// a crate that does everything else a word at a time. Taking only
    /// the crossings -- mask the span, then `trailing_zeros` down the
    /// set bits -- was measured and cost 0.51M instructions more. The
    /// mesh's seed runs are mostly short, so the span rarely reaches a
    /// second word and masking it costs more than reading the handful
    /// of bits it would have skipped.
    pub(crate) fn trim(&self, seed_run: &AreaRunSeed) -> Run {
        let crossing =
            if seed_run.is_column { &self.crossing_col } else { &self.crossing_row };
        let crossing = &crossing[seed_run.line as usize * CORNER_WORDS..];

        let run = seed_run.span();
        let (mut best, mut from) = (run, run.start);

        // A `u16`, because the run may end on the last cell of the line
        // and the walk goes one past its start.
        for pos in run.start as u16 + 1..=run.end as u16 {
            if crossing[pos as usize / 64] >> (pos % 64) & 1 == 0 {
                continue;
            }
            let piece = Run { start: from, end: pos as u8 - 1 };
            if from == run.start || piece.len() > best.len() {
                best = piece;
            }
            from = pos as u8;
        }

        let last = Run { start: from, end: run.end };
        if from != run.start && last.len() > best.len() {
            best = last;
        }
        best
    }
}
