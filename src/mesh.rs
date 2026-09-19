//! Greedy rectangle meshing: partitions the set bits of a [`BitMatrix`]
//! into a small number of non-overlapping rectangles.

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

    fn overlaps(&self, other: &Rect) -> bool {
        self.x0 <= other.x1 && other.x0 <= self.x1 && self.y0 <= other.y1 && other.y0 <= self.y1
    }

    /// Greedy pick order: bigger area wins; a tie goes to the squarer
    /// rectangle (closer width:height ratio, compared by cross
    /// multiplication to stay in integer math); a further tie (the same
    /// rectangle rotated, e.g. 1x3 vs 3x1) goes to the wider one.
    fn better_than(&self, other: &Rect) -> bool {
        let (area_self, area_other) = (self.area(), other.area());
        if area_self != area_other {
            return area_self > area_other;
        }

        let (min_self, max_self) = (
            self.width().min(self.height()) as u32,
            self.width().max(self.height()) as u32,
        );
        let (min_other, max_other) = (
            other.width().min(other.height()) as u32,
            other.width().max(other.height()) as u32,
        );
        let squareness = (min_self * max_other).cmp(&(min_other * max_self));
        if squareness != Ordering::Equal {
            return squareness == Ordering::Greater;
        }

        self.width() > other.width()
    }
}

/// A [`BitMatrix`] compressed into a list of non-overlapping rectangles
/// that exactly cover its set bits.
///
/// Built by scanning rows top to bottom. At each row, for the columns not
/// yet accounted for, it computes the best rectangle achievable *ignoring
/// any already-committed rectangles* (ignoring claims lets it "see" a
/// rectangle a still-growing earlier commitment is blocking), considering
/// only candidates that would cover at least one not-yet-claimed cell — a
/// candidate already fully covered is a subdivision of existing
/// rectangles, not a new area. If that
/// ideal candidate doesn't overlap anything yet claimed, it's committed
/// outright. If it does, and it's at least as large as the *combined*
/// area of every rectangle it would have to shrink, those rectangles are
/// clipped back to end just above the current row and the ideal candidate
/// takes over the freed cells; otherwise the clip isn't worth it and the
/// best rectangle using only the genuinely unclaimed cells is committed
/// instead.
///
/// This is a naive greedy heuristic, not a minimum-rectangle solver. It
/// favours large, square-ish rectangles rather than few of them, and it
/// cannot reach an optimum that requires taking a *smaller* rectangle
/// first, so it uses more rectangles than necessary on roughly a tenth
/// of all 4x4 bitmaps (see `examples/optimality_search.rs`). It is also
/// orientation-sensitive: the scan runs top to bottom and equal-area
/// ties prefer the wider rectangle, so the same shape can mesh better or
/// worse depending on how it is turned — feeding the search a shape's
/// eight symmetries and keeping the best drops that tenth to a fortieth,
/// which measures how much the row-major bias costs. A true
/// minimum partition into disjoint rectangles is not out of reach — it
/// is polynomial, via maximum matching over the chords joining reflex
/// vertices — it is just a different algorithm than this one. (Minimum
/// *cover* by rectangles allowed to overlap is the NP-hard variant.)
pub struct RectMesh {
    rects: Vec<Rect>,
}

impl RectMesh {
    pub fn from_bit_matrix(source: &BitMatrix) -> Self {
        let mut best = Mesher::run(source, None);

        // Every declined candidate is a decision the scan had to make
        // without knowing what came below it. Re-run the whole mesh with
        // each one pinned in place, which lets the rows above it settle
        // around the shape instead of against it, and keep whichever
        // attempt needs the fewest rectangles.
        for candidate in best.declined.clone() {
            let attempt = Mesher::run(source, Some(candidate));
            if attempt.rects.len() < best.rects.len() {
                best = attempt;
            }
        }

        Self { rects: best.rects }
    }

    /// Answers the same question as `BitMatrix::get`, by checking which
    /// (if any) rectangle covers `(x, y)`.
    pub fn get(&self, x: u8, y: u8) -> bool {
        self.rects.iter().any(|r| r.contains(x, y))
    }

    pub fn rects(&self) -> &[Rect] {
        &self.rects
    }
}

/// One top-to-bottom meshing attempt.
///
/// An attempt may be given a rectangle to `assume`: it starts already
/// committed and is held fixed, so the rows above it settle around it
/// rather than clipping it away. Without that protection the assumption
/// is pointless — an earlier row's candidate reaching down into it would
/// simply take those cells back, reproducing the original result.
struct Mesher<'a> {
    source: &'a BitMatrix,
    claimed: BitMatrix,
    rects: Vec<Rect>,
    /// Rectangles at indices below this are assumed and cannot be clipped.
    fixed: usize,
    /// Candidates this attempt turned down, each one a decision the scan
    /// had to make before seeing what lay below it.
    declined: Vec<Rect>,
}

impl<'a> Mesher<'a> {
    fn run(source: &'a BitMatrix, assume: Option<Rect>) -> Self {
        let mut mesher = Mesher {
            source,
            claimed: BitMatrix::new(),
            rects: Vec::new(),
            fixed: 0,
            declined: Vec::new(),
        };

        if let Some(rect) = assume {
            mesher.commit(rect);
            mesher.fixed = 1;
        }

        for row in 0..=u8::MAX {
            mesher.process_range(row, 0, u8::MAX);
        }

        mesher
    }

    fn commit(&mut self, rect: Rect) {
        self.claimed.set_rect(rect.x0 as i64, rect.y0 as i64, rect.x1 as i64, rect.y1 as i64);
        self.rects.push(rect);
    }

    /// Resolves the row-run(s) within `[col_lo, col_hi]` at `row`, then
    /// recurses into whatever column ranges are left on either side.
    fn process_range(&mut self, row: u8, col_lo: u8, col_hi: u8) {
        if col_lo > col_hi {
            return;
        }

        let source = self.source;
        let ideal = {
            let claimed = &self.claimed;
            largest_rect_from_row(
                |x, y| source.get(x, y),
                |x, y| !claimed.get(x, y),
                row,
                col_lo,
                col_hi,
            )
        };
        let Some(ideal) = ideal else {
            return;
        };

        let overlapping: Vec<usize> = self
            .rects
            .iter()
            .enumerate()
            .filter(|(_, r)| r.overlaps(&ideal))
            .map(|(i, _)| i)
            .collect();

        // Weigh the new rectangle against everything it would disturb,
        // not just the biggest piece: clipping several rectangles to gain
        // one no larger than their total only shatters them for nothing.
        let disturbed_area: u32 = overlapping.iter().map(|&i| self.rects[i].area()).sum();
        let touches_fixed = overlapping.iter().any(|&i| i < self.fixed);
        let commit_ideal = overlapping.is_empty()
            || (!touches_fixed && ideal.area() >= disturbed_area);

        if commit_ideal {
            self.clip_all(&overlapping, row);
            self.commit(ideal);
            self.recurse_around(row, col_lo, col_hi, ideal);
            return;
        }

        self.declined.push(ideal);

        let fallback = {
            let claimed = &self.claimed;
            largest_rect_from_row(
                |x, y| source.get(x, y) && !claimed.get(x, y),
                |_, _| true,
                row,
                col_lo,
                col_hi,
            )
        };
        let Some(fallback) = fallback else {
            return;
        };

        self.commit(fallback);
        self.recurse_around(row, col_lo, col_hi, fallback);
    }

    fn clip_all(&mut self, overlapping: &[usize], row: u8) {
        let mut to_remove = Vec::new();
        for &i in overlapping {
            let old = self.rects[i];
            self.claimed.unset_rect(old.x0 as i64, old.y0 as i64, old.x1 as i64, old.y1 as i64);
            if row > old.y0 {
                self.rects[i].y1 = row - 1;
                let clipped = self.rects[i];
                self.claimed.set_rect(
                    clipped.x0 as i64,
                    clipped.y0 as i64,
                    clipped.x1 as i64,
                    clipped.y1 as i64,
                );
            } else {
                to_remove.push(i);
            }
        }
        to_remove.sort_unstable_by(|a, b| b.cmp(a));
        for i in to_remove {
            self.rects.remove(i);
        }
    }

    fn recurse_around(&mut self, row: u8, col_lo: u8, col_hi: u8, committed: Rect) {
        if committed.x0 > col_lo {
            self.process_range(row, col_lo, committed.x0 - 1);
        }
        if committed.x1 < col_hi {
            self.process_range(row, committed.x1 + 1, col_hi);
        }
    }
}

/// Finds the largest rectangle whose top edge is `row`, within
/// `[col_lo, col_hi]`, where a column's available height is however many
/// consecutive rows starting at `row` satisfy `is_set`. Standard
/// largest-rectangle-in-histogram technique (monotonic stack), just
/// rooted at a single row instead of accumulated across many.
///
/// Only rectangles covering at least one cell satisfying `is_gain` are
/// considered, so a caller can rule out candidates that would merely
/// subdivide cells some existing rectangle already covers.
fn largest_rect_from_row(
    is_set: impl Fn(u8, u8) -> bool,
    is_gain: impl Fn(u8, u8) -> bool,
    row: u8,
    col_lo: u8,
    col_hi: u8,
) -> Option<Rect> {
    let width = col_hi as usize - col_lo as usize + 1;
    let mut heights = vec![0u16; width];
    // Per column, how far below `row` the first `is_gain` cell sits, so a
    // candidate of height h gains something iff some column it spans has
    // a value below h. u16::MAX means the column offers nothing new.
    let mut first_gain = vec![u16::MAX; width];

    for i in 0..width {
        let x = col_lo + i as u8;
        let mut h: u16 = 0;
        let mut y = row;
        while is_set(x, y) {
            if first_gain[i] == u16::MAX && is_gain(x, y) {
                first_gain[i] = h;
            }
            h += 1;
            if y == u8::MAX {
                break;
            }
            y += 1;
        }
        heights[i] = h;
    }

    let mut best: Option<Rect> = None;
    let mut stack: Vec<(usize, u16)> = Vec::new();
    // Runs one past `width` as a sentinel (height 0) to flush the stack.
    #[allow(clippy::needless_range_loop)]
    for i in 0..=width {
        let h = if i < width { heights[i] } else { 0 };
        let mut start = i;
        while let Some(&(s, sh)) = stack.last() {
            if sh <= h {
                break;
            }
            stack.pop();
            let candidate = Rect {
                x0: col_lo + s as u8,
                y0: row,
                x1: col_lo + (i - 1) as u8,
                y1: row + (sh - 1) as u8,
            };
            let gains_something = first_gain[s..i].iter().any(|&g| g < sh);
            if gains_something && best.as_ref().is_none_or(|b| candidate.better_than(b)) {
                best = Some(candidate);
            }
            start = s;
        }
        if h > 0 {
            stack.push((start, h));
        }
    }

    best
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

    fn assert_round_trip(bits: &BitMatrix, mesh: &RectMesh) {
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                assert_eq!(bits.get(x, y), mesh.get(x, y), "mismatch at ({x}, {y})");
            }
        }
    }

    fn assert_no_overlaps(mesh: &RectMesh) {
        for a in 0..mesh.rects().len() {
            for b in (a + 1)..mesh.rects().len() {
                let (ra, rb) = (mesh.rects()[a], mesh.rects()[b]);
                assert!(!ra.overlaps(&rb), "rects {a} and {b} overlap: {ra:?} {rb:?}");
            }
        }
    }

    #[test]
    fn empty_matrix_meshes_to_nothing() {
        let bits = BitMatrix::new();
        let mesh = RectMesh::from_bit_matrix(&bits);
        assert_eq!(mesh.rects().len(), 0);
        assert_round_trip(&bits, &mesh);
    }

    #[test]
    fn full_matrix_meshes_to_one_rect() {
        let mut bits = BitMatrix::new();
        bits.set_rect(0, 0, 255, 255);
        let mesh = RectMesh::from_bit_matrix(&bits);
        assert_eq!(mesh.rects(), &[Rect { x0: 0, y0: 0, x1: 255, y1: 255 }]);
        assert_round_trip(&bits, &mesh);
    }

    #[test]
    fn single_rectangle_round_trips_exactly() {
        let mut bits = BitMatrix::new();
        bits.set_rect(10, 20, 40, 30);
        let mesh = RectMesh::from_bit_matrix(&bits);
        assert_eq!(mesh.rects(), &[Rect { x0: 10, y0: 20, x1: 40, y1: 30 }]);
        assert_round_trip(&bits, &mesh);
    }

    #[test]
    fn area_tie_between_rotations_prefers_wider() {
        // An "L" shape: a 3-wide top row and a 3-tall left column,
        // sharing corner (0,0), each with area 3 and nothing bigger
        // available. The wider (top row) rectangle should be picked
        // over its 1x3 rotation.
        let mut bits = BitMatrix::new();
        bits.set(0, 0);
        bits.set(1, 0);
        bits.set(2, 0);
        bits.set(0, 1);
        bits.set(0, 2);

        let mesh = RectMesh::from_bit_matrix(&bits);
        assert_eq!(mesh.rects().len(), 2);
        assert!(mesh.rects().contains(&Rect { x0: 0, y0: 0, x1: 2, y1: 0 }));
        assert!(mesh.rects().contains(&Rect { x0: 0, y0: 1, x1: 0, y1: 2 }));
        assert_round_trip(&bits, &mesh);
    }

    #[test]
    fn area_tie_prefers_squarer_over_wider() {
        // A 6x1 top row and a 2x3 left block both have area 6 and
        // nothing bigger is available; the squarer 2x3 block should win
        // even though the row is wider.
        let mut bits = BitMatrix::new();
        bits.set_rect(0, 0, 5, 0);
        bits.set_rect(0, 1, 1, 2);

        let mesh = RectMesh::from_bit_matrix(&bits);
        assert_eq!(mesh.rects().len(), 2);
        assert!(mesh.rects().contains(&Rect { x0: 0, y0: 0, x1: 1, y1: 2 }));
        assert!(mesh.rects().contains(&Rect { x0: 2, y0: 0, x1: 5, y1: 0 }));
        assert_round_trip(&bits, &mesh);
    }

    #[test]
    fn disjoint_regions_and_circle_round_trip_with_no_overlap() {
        let mut bits = BitMatrix::new();
        bits.set_rect(10, 10, 40, 30);
        bits.set_circle(180, 180, 25);
        bits.unset_rect(20, 15, 30, 25);
        bits.unset_circle(180, 180, 8);

        let mesh = RectMesh::from_bit_matrix(&bits);
        assert_round_trip(&bits, &mesh);
        assert_no_overlaps(&mesh);
    }

    /// A rectangle already fully covered is not a candidate at all, so it
    /// cannot mask a smaller one that does add coverage. Here row 4's
    /// biggest ignore-claims rectangle is rows 4-7 of the 4x6 block (24
    /// cells), but every cell of it is already covered by that block, so
    /// the real pick is the 2x4 spanning cols 5-6, which is big enough
    /// (8 >= 5) to clip col 5's strip back to its first row.
    #[test]
    fn fully_covered_candidate_cannot_mask_a_smaller_real_one() {
        let mut bits = BitMatrix::new();
        bits.set_rect(0, 2, 3, 7);
        bits.set_rect(5, 3, 5, 7);
        bits.set_rect(6, 4, 6, 7);

        let mesh = RectMesh::from_bit_matrix(&bits);
        assert_round_trip(&bits, &mesh);
        assert_no_overlaps(&mesh);

        assert_eq!(mesh.rects().len(), 3);
        assert!(mesh.rects().contains(&Rect { x0: 0, y0: 2, x1: 3, y1: 7 }));
        assert!(mesh.rects().contains(&Rect { x0: 5, y0: 4, x1: 6, y1: 7 }));
        assert!(mesh.rects().contains(&Rect { x0: 5, y0: 3, x1: 5, y1: 3 }));
    }

    /// Found by exhaustive search as a worst case for the plain scan,
    /// which produced 5 rectangles where 3 suffice. Reaching 3 means
    /// declining the area-3 vertical bar at row 0 in favour of the area-2
    /// horizontal pair — a strictly smaller rectangle — which greedy will
    /// never do on its own. It gets there by re-running with row 2's
    /// turned-down 3x1 assumed, which blocks the bar from growing.
    #[test]
    fn adversarial_case_needs_a_smaller_first_rectangle() {
        let bits = bits_from_rows([
            [1, 1, 0, 0],
            [0, 1, 1, 1],
            [1, 1, 1, 0],
            [0, 0, 0, 0],
        ]);

        let mesh = RectMesh::from_bit_matrix(&bits);
        assert_round_trip(&bits, &mesh);
        assert_no_overlaps(&mesh);

        assert_eq!(mesh.rects().len(), 3);
        assert!(mesh.rects().contains(&Rect { x0: 0, y0: 0, x1: 1, y1: 0 }));
        assert!(mesh.rects().contains(&Rect { x0: 1, y0: 1, x1: 3, y1: 1 }));
        assert!(mesh.rects().contains(&Rect { x0: 0, y0: 2, x1: 2, y1: 2 }));
    }

    /// The worked 8x8 example: a top-to-bottom scan that clips a couple of
    /// earlier picks to make room for squarer combined rectangles found on
    /// later rows, but only when the new pick is at least as big as the
    /// largest rectangle it would have to shrink.
    #[test]
    fn worked_example_matches_expected_partition() {
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

        let mesh = RectMesh::from_bit_matrix(&bits);
        assert_round_trip(&bits, &mesh);
        assert_no_overlaps(&mesh);

        let expected = [
            Rect { x0: 3, y0: 0, x1: 3, y1: 3 }, // area 1, clipped to make room for area 7/A
            Rect { x0: 0, y0: 0, x1: 2, y1: 0 }, // area 2
            Rect { x0: 5, y0: 0, x1: 7, y1: 2 }, // area 3
            Rect { x0: 0, y0: 1, x1: 0, y1: 2 }, // area 4
            Rect { x0: 1, y0: 2, x1: 2, y1: 2 }, // area 5
            Rect { x0: 7, y0: 3, x1: 7, y1: 4 }, // area 6, clipped into B
            Rect { x0: 3, y0: 4, x1: 4, y1: 4 }, // area 7, clipped into A
            Rect { x0: 3, y0: 5, x1: 7, y1: 7 }, // area 9
            Rect { x0: 0, y0: 6, x1: 1, y1: 7 }, // area C
            Rect { x0: 2, y0: 6, x1: 2, y1: 6 }, // area D
        ];
        assert_eq!(mesh.rects().len(), expected.len());
        for rect in expected {
            assert!(mesh.rects().contains(&rect), "missing expected rect {rect:?}");
        }
    }

    /// The worst case for this greedy algorithm: no two set cells share
    /// an edge, so every rectangle is 1x1.
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

        let mesh = RectMesh::from_bit_matrix(&bits);
        assert_eq!(mesh.rects().len(), 32768);
        assert_round_trip(&bits, &mesh);
        assert_no_overlaps(&mesh);
    }
}
