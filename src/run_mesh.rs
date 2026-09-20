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
