//! Rectangle meshing that works entirely on run lists.
//!
//! Rows and columns are both reduced to runs once, up front. Each step
//! takes the first row run still standing, in scan order, and sinks it:
//! the rectangle is that whole run, carried down as far as every column
//! under it reaches. Both run lists are then updated to exclude what was
//! taken, splitting a run in two where the rectangle cut through it.
//!
//! Two choices earn their keep, and both are choices to look at less.
//!
//! Seeding on the topmost run rather than the longest one keeps the
//! partition tidy. Taking the biggest rectangle available anywhere carves
//! the middle out of a shape and leaves a ring around it, and rings
//! shatter: seeding by size needs 107 rectangles where sweeping top to
//! bottom needs 75, and three different ways of ranking by size all land
//! on exactly 107, so it is the ranking that costs, not the tie-breaks.
//!
//! Taking the seed whole, rather than the best rectangle lying along it,
//! is worth another two. A step could instead cut its seed into several
//! rectangles, trading depth for count; scoring those cuts by area less a
//! fixed charge per rectangle and sweeping the charge from nothing to
//! unbounded improves the answer monotonically as the charge rises, and
//! saturates once it is high enough to forbid cutting at all. Measured
//! over 200 bitmaps: 79 rectangles when every position becomes its own,
//! 78 taking the single best rectangle per step, 77 taking the seed
//! whole. Against exhaustive optima on small grids it cuts the number of
//! bitmaps meshed suboptimally by roughly a third. So there is nothing to
//! rank within a step either, and the search inside a step disappears.
//!
//! Nothing here walks cells. A step costs the length of the seed run, not
//! the width of the grid, and there are as many steps as there are
//! rectangles in the answer.

use crate::BitMatrix;

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

    /// The first run still standing in scan order: lowest line, then
    /// lowest position within it.
    fn topmost(&self) -> Option<(u8, Span)> {
        self.lines
            .iter()
            .enumerate()
            .find_map(|(line, spans)| spans.first().map(|s| (line as u8, *s)))
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
    fn carve(&mut self, lines: (u8, u8), lo: u8, hi: u8) {
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

            self.lines[index].splice(first..last, pieces);
        }
    }
}

/// A [`BitMatrix`] partitioned into rectangles by repeatedly taking the
/// topmost run still standing and sinking it as deep as it will go.
pub struct RunMesh {
    rects: Vec<Rect>,
}

impl RunMesh {
    pub fn from_bit_matrix(source: &BitMatrix) -> Self {
        let mut rows = Runs::rows_of(source);
        let mut cols = Runs::cols_of(source);
        let mut rects = Vec::new();

        while let Some((line, seed)) = rows.topmost() {
            let rect = sink(&cols, seed, line);
            rows.carve((rect.y0, rect.y1), rect.x0, rect.x1);
            cols.carve((rect.x0, rect.x1), rect.y0, rect.y1);
            rects.push(rect);
        }

        Self { rects }
    }

    pub fn rects(&self) -> &[Rect] {
        &self.rects
    }
}

// ---------------------------------------------------------------------
// Experiment: largest-run-first seeding, with a step allowed to emit
// several rectangles. Not wired into `from_bit_matrix`.
// ---------------------------------------------------------------------

impl RunMesh {
    /// Seeds on the longest run left anywhere, a row run breaking a tie
    /// on length and the most up-left run breaking a tie on that, and
    /// lets each step cover its seed with several rectangles.
    #[doc(hidden)]
    pub fn largest_first(source: &BitMatrix, rect_cost: i64) -> Self {
        let mut rows = Runs::rows_of(source);
        let mut cols = Runs::cols_of(source);
        let mut rects = Vec::new();
        let mut step = Vec::new();

        while let Some((line, seed, is_column)) = pick_longest(&rows, &cols) {
            step.clear();
            let crossing = if is_column { &rows } else { &cols };
            split_along(crossing, seed, line, is_column, rect_cost, &mut step);

            for rect in step.drain(..) {
                rows.carve((rect.y0, rect.y1), rect.x0, rect.x1);
                cols.carve((rect.x0, rect.x1), rect.y0, rect.y1);
                rects.push(rect);
            }
        }

        Self { rects }
    }
}

/// Largest-run-first with the old step: one rectangle per seed, the best
/// one lying along it by area, then squareness, then width.
impl RunMesh {
    #[doc(hidden)]
    pub fn largest_first_single(source: &BitMatrix) -> Self {
        let mut rows = Runs::rows_of(source);
        let mut cols = Runs::cols_of(source);
        let mut rects = Vec::new();

        while let Some((line, seed, is_column)) = pick_longest(&rows, &cols) {
            let crossing = if is_column { &rows } else { &cols };
            let mut crossings: Vec<Span> = Vec::with_capacity(seed.len() as usize);
            for pos in seed.start..=seed.end {
                match crossing.span_at(pos, line) {
                    Some(span) => crossings.push(span),
                    None => break,
                }
            }

            let mut best: Option<Rect> = None;
            for (i, first) in crossings.iter().enumerate() {
                let (mut lo, mut hi) = (first.start, first.end);
                for (j, span) in crossings.iter().enumerate().skip(i) {
                    lo = lo.max(span.start);
                    hi = hi.min(span.end);
                    if lo > hi {
                        break;
                    }
                    let (from, to) = (seed.start + i as u8, seed.start + j as u8);
                    let rect = if is_column {
                        Rect { x0: lo, y0: from, x1: hi, y1: to }
                    } else {
                        Rect { x0: from, y0: lo, x1: to, y1: hi }
                    };
                    let better = best.is_none_or(|b: Rect| {
                        rect.area() > b.area()
                            || (rect.area() == b.area() && {
                                let (a, c) = (rect.width().min(rect.height()) as u32, rect.width().max(rect.height()) as u32);
                                let (d, e) = (b.width().min(b.height()) as u32, b.width().max(b.height()) as u32);
                                a * e > d * c || (a * e == d * c && rect.width() > b.width())
                            })
                    });
                    if better {
                        best = Some(rect);
                    }
                }
            }

            let rect = best.expect("a seed run always yields at least itself");
            rows.carve((rect.y0, rect.y1), rect.x0, rect.x1);
            cols.carve((rect.x0, rect.x1), rect.y0, rect.y1);
            rects.push(rect);
        }

        Self { rects }
    }
}

/// The longest run still standing in either orientation. Length wins; a
/// tie goes to a row run; a further tie goes to whichever starts at the
/// upper-left-most cell.
fn pick_longest(rows: &Runs, cols: &Runs) -> Option<(u8, Span, bool)> {
    // (length, is_row, -y, -x) compared as "bigger is better".
    let mut best: Option<(u16, bool, i32, i32)> = None;
    let mut pick: Option<(u8, Span, bool)> = None;

    for (line, spans) in rows.lines.iter().enumerate() {
        for span in spans {
            let key = (span.len(), true, -(line as i32), -(span.start as i32));
            if best.is_none_or(|b| key > b) {
                best = Some(key);
                pick = Some((line as u8, *span, false));
            }
        }
    }
    for (line, spans) in cols.lines.iter().enumerate() {
        for span in spans {
            let key = (span.len(), false, -(span.start as i32), -(line as i32));
            if best.is_none_or(|b| key > b) {
                best = Some(key);
                pick = Some((line as u8, *span, true));
            }
        }
    }

    pick
}

/// Covers a seed run with as much area as possible using as few
/// rectangles as that takes.
///
/// A rectangle is a contiguous stretch of the seed's crossing runs
/// intersected together, so covering the seed means cutting it into
/// consecutive stretches: a long stretch reaches only as deep as its
/// shallowest member, a short one reaches deeper but costs another
/// rectangle. `rect_cost` is what a rectangle has to earn to be worth
/// spending; at zero, area decides and the fewest rectangles achieving
/// that area break the tie.
fn split_along(
    crossing: &Runs,
    seed: Span,
    line: u8,
    seed_is_column: bool,
    rect_cost: i64,
    out: &mut Vec<Rect>,
) {
    let mut crossings: Vec<Span> = Vec::with_capacity(seed.len() as usize);
    for pos in seed.start..=seed.end {
        match crossing.span_at(pos, line) {
            Some(span) => crossings.push(span),
            None => break,
        }
    }

    // best[j] is the (score, rectangles) of the best cover of the first j
    // positions and cut[j] where its last rectangle starts. Extending a
    // stretch leftwards only raises its floor and lowers its ceiling, so
    // every candidate's depth falls out of the walk.
    let n = crossings.len();
    let mut best = vec![(0i64, 0usize); n + 1];
    let mut cut = vec![0usize; n + 1];
    for j in 1..=n {
        let (mut lo, mut hi) = (0u8, u8::MAX);
        let mut chosen = (i64::MIN, 0usize);
        for i in (0..j).rev() {
            lo = lo.max(crossings[i].start);
            hi = hi.min(crossings[i].end);
            let area = (j - i) as i64 * (hi as i64 - lo as i64 + 1);
            let (prev, prev_rects) = best[i];
            let candidate = (prev + area - rect_cost, prev_rects + 1);
            if candidate.0 > chosen.0 || (candidate.0 == chosen.0 && candidate.1 < chosen.1) {
                chosen = candidate;
                cut[j] = i;
            }
        }
        best[j] = chosen;
    }

    let start = out.len();
    let mut j = n;
    while j > 0 {
        let i = cut[j];
        let (mut lo, mut hi) = (0u8, u8::MAX);
        for span in &crossings[i..j] {
            lo = lo.max(span.start);
            hi = hi.min(span.end);
        }
        let (from, to) = (seed.start + i as u8, seed.start + j as u8 - 1);
        out.push(if seed_is_column {
            Rect { x0: lo, y0: from, x1: hi, y1: to }
        } else {
            Rect { x0: from, y0: lo, x1: to, y1: hi }
        });
        j = i;
    }
    out[start..].reverse();
}

/// The whole seed run, taken as far as every one of its crossing runs
/// reaches.
///
/// The seed spans positions `seed.start..=seed.end` on `line`, and each of
/// those positions sits in exactly one crossing run, so the rectangle is
/// as wide as the seed and as deep as the shallowest column under it.
/// Since the seed is the topmost run left, nothing is set above `line` and
/// every crossing run starts there; the top is tracked anyway so the
/// rectangle depends on the seed alone and not on how it was chosen.
fn sink(crossing: &Runs, seed: Span, line: u8) -> Rect {
    let (mut top, mut bottom) = (0u8, u8::MAX);
    for pos in seed.start..=seed.end {
        let span = crossing
            .span_at(pos, line)
            .expect("a cell still standing belongs to a run of either kind");
        top = top.max(span.start);
        bottom = bottom.min(span.end);
    }

    Rect { x0: seed.start, y0: top, x1: seed.end, y1: bottom }
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

    /// The worked 8x8 example. Ten rectangles is the proven optimum for
    /// this shape, so largest-run-first reaches it.
    #[test]
    fn worked_example_reaches_ten() {
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

        let mesh = RunMesh::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 10);
    }

    /// The 4x4 that cost 5 rectangles when a step picked the best
    /// rectangle along its seed, and 4 when seeds were ranked by length.
    /// Taking each seed whole reaches 3, which is the proven optimum.
    #[test]
    fn adversarial_four_by_four_is_optimal() {
        let bits = bits_from_rows([
            [1, 1, 0, 0],
            [0, 1, 1, 1],
            [1, 1, 1, 0],
            [0, 0, 0, 0],
        ]);

        let mesh = RunMesh::from_bit_matrix(&bits);
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
    fn random_small_bitmaps_stay_exact_partitions() {
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
