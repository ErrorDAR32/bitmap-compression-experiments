//! Rectangle meshing that works entirely on run lists.
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

use crate::BitMatrix;
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

impl Runs {
    fn rows_of(source: &BitMatrix) -> Self {
        Self::build(|line, pos| source.get(pos, line))
    }

    fn cols_of(source: &BitMatrix) -> Self {
        Self::build(|line, pos| source.get(line, pos))
    }

    fn build(set: impl Fn(u8, u8) -> bool) -> Self {
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
        Self { lines }
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
pub struct RunMesh {
    rects: Vec<Rect>,
}

impl RunMesh {
    pub fn from_bit_matrix(source: &BitMatrix) -> Self {
        let mut rows = Runs::rows_of(source);
        let mut cols = Runs::cols_of(source);
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

        Self { rects }
    }

    pub fn rects(&self) -> &[Rect] {
        &self.rects
    }

    /// Rewrites the partition by giving rectangles away to their
    /// neighbours, and answers how many were reclaimed. See
    /// [`crate::mutate`] for what the moves are and what they cost.
    pub fn compact(&mut self) -> usize {
        crate::mutate::compact(&mut self.rects)
    }

    /// Only the free half of [`Self::compact`], which reclaims nothing on
    /// its own. Kept so that claim stays measurable.
    #[doc(hidden)]
    pub fn dissolve_only(&mut self) -> usize {
        crate::mutate::dissolve_only(&mut self.rects)
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

impl RunMesh {
    #[doc(hidden)]
    pub fn tie_break(source: &BitMatrix, prefer_least: bool) -> Self {
        let mut rows = Runs::rows_of(source);
        let mut cols = Runs::cols_of(source);
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

        Self { rects }
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

    fn bits_from_rows<const N: usize>(rows: [[u8; N]; N]) -> BitMatrix {
        let mut bits = BitMatrix::new();
        for (y, row) in rows.iter().enumerate() {
            for (x, &cell) in row.iter().enumerate() {
                if cell == 1 {
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
    fn assert_exact_partition(bits: &BitMatrix, mesh: &RunMesh) {
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
        assert_eq!(RunMesh::from_bit_matrix(&empty).rects().len(), 0);

        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        let mesh = RunMesh::from_bit_matrix(&full);
        assert_eq!(mesh.rects(), &[Rect { x0: 0, y0: 0, x1: 255, y1: 255 }]);
    }

    #[test]
    fn single_rectangle_comes_back_whole() {
        let mut bits = BitMatrix::new();
        bits.set_rect(10, 20, 40, 30);
        let mesh = RunMesh::from_bit_matrix(&bits);
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

        let mesh = RunMesh::from_bit_matrix(&bits);
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

        let mesh = RunMesh::from_bit_matrix(&bits);
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
        let bits = bits_from_rows([
            [1, 1, 1, 1, 0, 1, 1, 1],
            [1, 0, 0, 1, 0, 1, 1, 1],
            [1, 1, 1, 1, 0, 1, 1, 1],
            [0, 0, 0, 1, 0, 0, 0, 1],
            [0, 0, 0, 1, 1, 0, 0, 1],
            [0, 0, 0, 1, 1, 1, 1, 1],
            [1, 1, 1, 1, 1, 1, 1, 1],
            [1, 1, 0, 1, 1, 1, 1, 1],
        ]);

        let mut mesh = RunMesh::from_bit_matrix(&bits);
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
        let bits = bits_from_rows([
            [1, 1, 0, 0],
            [0, 1, 1, 1],
            [1, 1, 1, 0],
            [0, 0, 0, 0],
        ]);

        let mut mesh = RunMesh::from_bit_matrix(&bits);
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

        let mesh = RunMesh::from_bit_matrix(&bits);
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
                let mesh = RunMesh::from_bit_matrix(&bits);
                assert_exact_partition(&bits, &mesh);
            }
        }
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

        let mesh = RunMesh::from_bit_matrix(&bits);
        assert_eq!(mesh.rects().len(), 32768);
        assert_exact_partition(&bits, &mesh);
    }
}
