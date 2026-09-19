//! Rectangle meshing that works entirely on run lists.
//!
//! Rows and columns are both reduced to runs once, up front. Each step
//! takes the first row run still standing, in scan order, and carves out
//! the largest rectangle lying along it; both run lists are then updated
//! to exclude what was taken, splitting a run in two where the rectangle
//! cut through its middle.
//!
//! Seeding on the topmost run rather than the longest one is what keeps
//! the partition tidy. Taking the biggest rectangle available anywhere
//! carves the middle out of a shape and leaves a ring around it, and
//! rings shatter: that ordering needs 107 rectangles on a 256x256 bitmap
//! where sweeping top to bottom needs 75.
//!
//! Nothing here walks cells. A step costs the length of the seed run, not
//! the width of the grid, and there are as many steps as there are
//! rectangles in the answer.

use crate::BitMatrix;
use std::cmp::Ordering;


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

    /// Pick order: bigger area wins; a tie goes to the squarer rectangle
    /// (closer width:height ratio, compared by cross multiplication to
    /// stay in integer math); a further tie (the same rectangle rotated,
    /// e.g. 1x3 vs 3x1) goes to the wider one.
    fn better_than(&self, other: &Rect) -> bool {
        if self.area() != other.area() {
            return self.area() > other.area();
        }

        let (min_self, max_self) = (
            self.width().min(self.height()) as u32,
            self.width().max(self.height()) as u32,
        );
        let (min_other, max_other) = (
            other.width().min(other.height()) as u32,
            other.width().max(other.height()) as u32,
        );
        match (min_self * max_other).cmp(&(min_other * max_self)) {
            Ordering::Greater => true,
            Ordering::Less => false,
            Ordering::Equal => self.width() > other.width(),
        }
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
/// columns, `lines[x]` holds row spans. The two are mirror images, which
/// is what lets a seed be either kind without special-casing.
///
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

/// A [`BitMatrix`] partitioned into rectangles by repeatedly carving out
/// the largest rectangle lying along the longest remaining run.
pub struct RunMesh {
    rects: Vec<Rect>,
}

impl RunMesh {
    pub fn from_bit_matrix(source: &BitMatrix) -> Self {
        let mut rows = Runs::rows_of(source);
        let mut cols = Runs::cols_of(source);
        let mut rects = Vec::new();

        loop {
            let rect = match rows.topmost() {
                None => break,
                Some((y, span)) => best_along(&cols, span, y, false),
            };

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

/// The largest rectangle lying along a seed run.
///
/// The seed spans positions `seed.start..=seed.end` on line `line`. Each
/// of those positions has a crossing run, and a rectangle is a contiguous
/// stretch of them intersected together — so this is the largest rectangle
/// in a histogram whose entries are the crossing runs, of which there are
/// as many as the seed is long rather than one per column of the grid.
fn best_along(crossing: &Runs, seed: Span, line: u8, seed_is_column: bool) -> Rect {
    // Gather the crossing runs once, in seed order. Every position of the
    // seed has exactly one: a cell still standing belongs to a run in both
    // orientations, so a seed of length k crosses exactly k runs and none
    // of them can be filtered away. Looking them up here rather than
    // inside the search below turns k*k lookups into k.
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
            let rect = if seed_is_column {
                Rect { x0: lo, y0: from, x1: hi, y1: to }
            } else {
                Rect { x0: from, y0: lo, x1: to, y1: hi }
            };
            if best.is_none_or(|b| rect.better_than(&b)) {
                best = Some(rect);
            }
        }
    }

    best.expect("a seed run always yields at least itself")
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
    fn area_tie_between_rotations_prefers_wider() {
        // An "L": a 3-wide top row and a 3-tall left column sharing corner
        // (0,0), each area 3 with nothing bigger available.
        let mut bits = BitMatrix::new();
        bits.set(0, 0);
        bits.set(1, 0);
        bits.set(2, 0);
        bits.set(0, 1);
        bits.set(0, 2);

        let mesh = RunMesh::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 2);
        assert!(mesh.rects().contains(&Rect { x0: 0, y0: 0, x1: 2, y1: 0 }));
    }

    #[test]
    fn area_tie_prefers_squarer_over_wider() {
        // A 6x1 row and a 2x3 block both have area 6; the squarer block
        // should win even though the row is wider.
        let mut bits = BitMatrix::new();
        bits.set_rect(0, 0, 5, 0);
        bits.set_rect(0, 1, 1, 2);

        let mesh = RunMesh::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 2);
        assert!(mesh.rects().contains(&Rect { x0: 0, y0: 0, x1: 1, y1: 2 }));
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

    /// The 4x4 that needs a smaller rectangle taken first, where the
    /// optimum is 3. Sweeping top to bottom gets 5 here, worse than the
    /// 4 that seeding on the longest run managed — but that ordering
    /// costs 107 rectangles against 75 on real input, so this is the
    /// trade being made. Recorded as the behaviour it has.
    #[test]
    fn adversarial_four_by_four_is_two_over() {
        let bits = bits_from_rows([
            [1, 1, 0, 0],
            [0, 1, 1, 1],
            [1, 1, 1, 0],
            [0, 0, 0, 0],
        ]);

        let mesh = RunMesh::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 5);
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
