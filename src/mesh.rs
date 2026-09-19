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
/// outright. If it does, and it's at least as large as the biggest
/// rectangle it would have to shrink, the blocking rectangle(s) are
/// clipped back to end just above the current row and the ideal candidate
/// takes over the freed cells; otherwise the clip isn't worth it and the
/// best rectangle using only the genuinely unclaimed cells is committed
/// instead.
///
/// This is a naive greedy heuristic, not a minimum-rectangle solver. It
/// favours large, square-ish rectangles rather than few of them, and it
/// cannot reach an optimum that requires taking a *smaller* rectangle
/// first, so it uses more rectangles than necessary on roughly a tenth
/// of all 4x4 bitmaps (see `examples/optimality_search.rs`). A true
/// minimum partition into disjoint rectangles is not out of reach — it
/// is polynomial, via maximum matching over the chords joining reflex
/// vertices — it is just a different algorithm than this one. (Minimum
/// *cover* by rectangles allowed to overlap is the NP-hard variant.)
pub struct RectMesh {
    rects: Vec<Rect>,
}

impl RectMesh {
    /// Meshes the shape in all eight orientations of the square and keeps
    /// whichever needs the fewest rectangles. The scan runs top to bottom
    /// and ties prefer the wider rectangle, so turning the shape can
    /// produce a genuinely better partition — it roughly halves how often
    /// the result is suboptimal, twice over. Orientations are tried with
    /// the untransformed one first and only replaced on a strict
    /// improvement, so a tie always keeps the untransformed result.
    pub fn from_bit_matrix(source: &BitMatrix) -> Self {
        let mut best: Option<Vec<Rect>> = None;

        for sym in 0..8u8 {
            let rects: Vec<Rect> = mesh_once(&reorient(source, sym))
                .into_iter()
                .map(|r| unmap_rect(sym, r))
                .collect();
            if best.as_ref().is_none_or(|b| rects.len() < b.len()) {
                best = Some(rects);
            }
        }

        Self { rects: best.unwrap_or_default() }
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

/// Maps a point under one of the eight symmetries of the square grid:
/// identity, three rotations, both diagonal flips, and both axis flips.
fn map_point(sym: u8, x: u8, y: u8) -> (u8, u8) {
    const M: u8 = u8::MAX;
    match sym {
        0 => (x, y),
        1 => (M - y, x),
        2 => (M - x, M - y),
        3 => (y, M - x),
        4 => (y, x),
        5 => (M - y, M - x),
        6 => (M - x, y),
        _ => (x, M - y),
    }
}

/// Every symmetry is its own inverse except the two quarter turns, which
/// invert into each other.
fn inverse(sym: u8) -> u8 {
    match sym {
        1 => 3,
        3 => 1,
        other => other,
    }
}

fn reorient(source: &BitMatrix, sym: u8) -> BitMatrix {
    let mut out = BitMatrix::new();
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if source.get(x, y) {
                let (nx, ny) = map_point(sym, x, y);
                out.set(nx, ny);
            }
        }
    }
    out
}

/// These symmetries take axis-aligned rectangles to axis-aligned
/// rectangles, so mapping two opposite corners back and renormalizing
/// recovers the rectangle in the original orientation.
fn unmap_rect(sym: u8, rect: Rect) -> Rect {
    let back = inverse(sym);
    let (ax, ay) = map_point(back, rect.x0, rect.y0);
    let (bx, by) = map_point(back, rect.x1, rect.y1);
    Rect {
        x0: ax.min(bx),
        y0: ay.min(by),
        x1: ax.max(bx),
        y1: ay.max(by),
    }
}

fn mesh_once(source: &BitMatrix) -> Vec<Rect> {
    let mut claimed = BitMatrix::new();
    let mut rects: Vec<Rect> = Vec::new();
    for row in 0..=u8::MAX {
        process_range(source, &mut claimed, &mut rects, row, 0, u8::MAX);
    }
    rects
}

/// Resolves the row-run(s) within `[col_lo, col_hi]` at `row`, committing
/// or clipping rectangles as described on [`RectMesh`], then recurses into
/// whatever column ranges are left on either side of what it just decided.
fn process_range(
    source: &BitMatrix,
    claimed: &mut BitMatrix,
    rects: &mut Vec<Rect>,
    row: u8,
    col_lo: u8,
    col_hi: u8,
) {
    if col_lo > col_hi {
        return;
    }

    let Some(ideal) = largest_rect_from_row(
        |x, y| source.get(x, y),
        |x, y| !claimed.get(x, y),
        row,
        col_lo,
        col_hi,
    ) else {
        return;
    };

    let overlapping: Vec<usize> = rects
        .iter()
        .enumerate()
        .filter(|(_, r)| r.overlaps(&ideal))
        .map(|(i, _)| i)
        .collect();

    // Weigh the new rectangle against everything it would disturb, not
    // just the biggest piece: clipping several rectangles to gain one no
    // larger than their total only shatters them for nothing.
    let disturbed_area: u32 = overlapping.iter().map(|&i| rects[i].area()).sum();
    let should_commit_ideal = overlapping.is_empty() || ideal.area() >= disturbed_area;

    if should_commit_ideal {
        let mut to_remove = Vec::new();
        for &i in &overlapping {
            let old = rects[i];
            claimed.unset_rect(old.x0 as i64, old.y0 as i64, old.x1 as i64, old.y1 as i64);
            if row > old.y0 {
                rects[i].y1 = row - 1;
                let clipped = rects[i];
                claimed.set_rect(
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
            rects.remove(i);
        }

        commit(claimed, rects, ideal);
        recurse_around(source, claimed, rects, row, col_lo, col_hi, ideal);
        return;
    }

    let Some(fallback) = largest_rect_from_row(
        |x, y| source.get(x, y) && !claimed.get(x, y),
        |_, _| true,
        row,
        col_lo,
        col_hi,
    ) else {
        return;
    };

    commit(claimed, rects, fallback);
    recurse_around(source, claimed, rects, row, col_lo, col_hi, fallback);
}

fn commit(claimed: &mut BitMatrix, rects: &mut Vec<Rect>, rect: Rect) {
    claimed.set_rect(rect.x0 as i64, rect.y0 as i64, rect.x1 as i64, rect.y1 as i64);
    rects.push(rect);
}

fn recurse_around(
    source: &BitMatrix,
    claimed: &mut BitMatrix,
    rects: &mut Vec<Rect>,
    row: u8,
    col_lo: u8,
    col_hi: u8,
    committed: Rect,
) {
    if committed.x0 > col_lo {
        process_range(source, claimed, rects, row, col_lo, committed.x0 - 1);
    }
    if committed.x1 < col_hi {
        process_range(source, claimed, rects, row, committed.x1 + 1, col_hi);
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
