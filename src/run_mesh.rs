//! Largest-run-first meshing, working entirely on run lists.
//!
//! Rows and columns are both reduced to runs once, up front. Each step
//! takes the longest run still standing, looks at the runs crossing it,
//! and carves out the largest rectangle lying along it; both run lists
//! are then updated to exclude what was taken, splitting a run in two
//! where the rectangle cut through its middle.
//!
//! Nothing here walks cells or rows. A step costs the length of the seed
//! run, not the width of the grid, and there are as many steps as there
//! are rectangles in the answer.

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

    pub fn contains(&self, x: u8, y: u8) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }

    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x0 <= other.x1 && other.x0 <= self.x1 && self.y0 <= other.y1 && other.y0 <= self.y1
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

    /// Scans every run, so a step costs O(runs remaining). Fine while the
    /// run count stays in the hundreds; a shape that fragments into
    /// thousands of single cells would want a heap here instead.
    fn longest(&self) -> Option<(u8, Span)> {
        let mut best: Option<(u8, Span)> = None;
        for (line, spans) in self.lines.iter().enumerate() {
            for span in spans {
                if best.is_none_or(|(_, b)| span.len() > b.len()) {
                    best = Some((line as u8, *span));
                }
            }
        }
        best
    }

    fn span_at(&self, line: u8, pos: u8) -> Option<Span> {
        self.lines[line as usize]
            .iter()
            .copied()
            .find(|s| pos >= s.start && pos <= s.end)
    }

    /// Removes `[lo, hi]` from every line in `lines`, splitting any span
    /// the range cuts through.
    fn carve(&mut self, lines: (u8, u8), lo: u8, hi: u8) {
        for line in lines.0..=lines.1 {
            let spans = &mut self.lines[line as usize];
            let mut next = Vec::with_capacity(spans.len() + 1);
            for span in spans.iter() {
                if hi < span.start || lo > span.end {
                    next.push(*span);
                    continue;
                }
                if lo > span.start {
                    next.push(Span { start: span.start, end: lo - 1 });
                }
                if hi < span.end {
                    next.push(Span { start: hi + 1, end: span.end });
                }
            }
            *spans = next;
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
            let row_seed = rows.longest();
            let col_seed = cols.longest();

            // A tie goes to the column seed, matching the worked example;
            // on that shape either choice yields the same rectangle.
            let rect = match (row_seed, col_seed) {
                (None, None) => break,
                (Some((y, span)), None) => best_along(&cols, span, y, false),
                (None, Some((x, span))) => best_along(&rows, span, x, true),
                (Some((y, r)), Some((x, c))) => {
                    if c.len() >= r.len() {
                        best_along(&rows, c, x, true)
                    } else {
                        best_along(&cols, r, y, false)
                    }
                }
            };

            rows.carve((rect.y0, rect.y1), rect.x0, rect.x1);
            cols.carve((rect.x0, rect.x1), rect.y0, rect.y1);
            rects.push(rect);
        }

        Self { rects }
    }

    /// Answers the same question as `BitMatrix::get`.
    pub fn get(&self, x: u8, y: u8) -> bool {
        self.rects.iter().any(|r| r.contains(x, y))
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
    let mut best: Option<Rect> = None;

    for from in seed.start..=seed.end {
        let (mut lo, mut hi) = (0u8, u8::MAX);
        for to in from..=seed.end {
            let Some(span) = crossing.span_at(to, line) else {
                break;
            };
            lo = lo.max(span.start);
            hi = hi.min(span.end);
            if lo > hi {
                break;
            }

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
    fn assert_exact_partition(bits: &BitMatrix, mesh: &RunMesh) {
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                assert_eq!(bits.get(x, y), mesh.get(x, y), "mismatch at ({x}, {y})");
            }
        }
        for a in 0..mesh.rects().len() {
            for b in (a + 1)..mesh.rects().len() {
                let (ra, rb) = (mesh.rects()[a], mesh.rects()[b]);
                assert!(!ra.overlaps(&rb), "rects {a} and {b} overlap: {ra:?} {rb:?}");
            }
        }
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

    /// The 4x4 that needs a smaller rectangle taken first. Largest-run-
    /// first gets 4 against an optimum of 3: seeded on the 3-long column
    /// run it finds the 2x2 (area 4) and takes it over the 1x3 (area 3).
    /// Recorded as the behaviour it has, not the behaviour hoped for.
    #[test]
    fn adversarial_four_by_four_is_still_one_over() {
        let bits = bits_from_rows([
            [1, 1, 0, 0],
            [0, 1, 1, 1],
            [1, 1, 1, 0],
            [0, 0, 0, 0],
        ]);

        let mesh = RunMesh::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 4);
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
    /// long and nothing ever merges. Also where scanning every run to find
    /// the longest turns quadratic.
    #[test]
    #[ignore]
    fn checkerboard_stress_test() {
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
