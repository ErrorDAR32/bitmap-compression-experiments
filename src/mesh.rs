//! Greedy rectangle meshing: partitions the set bits of a [`BitMatrix`]
//! into a small number of non-overlapping rectangles.

use crate::{BitMatrix, WIDTH};
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
/// that exactly cover its set bits, built by repeatedly carving out the
/// largest remaining all-set rectangle.
///
/// This is a naive greedy heuristic, not a minimum-rectangle-count
/// solver (that problem is NP-hard) — each of its O(rects) passes over
/// the grid is itself only O(width * height), so pathological inputs
/// that force many tiny rectangles (e.g. a checkerboard) are slow.
pub struct RectMesh {
    rects: Vec<Rect>,
}

impl RectMesh {
    pub fn from_bit_matrix(source: &BitMatrix) -> Self {
        let mut covered = BitMatrix::new();
        let mut rects = Vec::new();

        while let Some(rect) = largest_uncovered_rect(source, &covered) {
            covered.set_rect(rect.x0 as i64, rect.y0 as i64, rect.x1 as i64, rect.y1 as i64);
            rects.push(rect);
        }

        Self { rects }
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

/// Finds the largest rectangle of cells that are set in `source` and not
/// yet set in `covered`, using the row-by-row histogram technique (a
/// monotonic stack computing the largest rectangle in each row's
/// histogram of consecutive-uncovered-set-cell run heights).
fn largest_uncovered_rect(source: &BitMatrix, covered: &BitMatrix) -> Option<Rect> {
    let mut heights = [0u16; WIDTH];
    let mut best: Option<Rect> = None;

    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            let xi = x as usize;
            if source.get(x, y) && !covered.get(x, y) {
                heights[xi] += 1;
            } else {
                heights[xi] = 0;
            }
        }

        let mut stack: Vec<(usize, u16)> = Vec::new();
        // Runs one past WIDTH as a sentinel (height 0) to flush the stack,
        // so this can't be an `iter().enumerate()` over `heights`.
        #[allow(clippy::needless_range_loop)]
        for x in 0..=WIDTH {
            let h = if x < WIDTH { heights[x] } else { 0 };
            let mut start = x;
            while let Some(&(s, sh)) = stack.last() {
                if sh <= h {
                    break;
                }
                stack.pop();
                let candidate = Rect {
                    x0: s as u8,
                    y0: (y as u16 + 1 - sh) as u8,
                    x1: (x - 1) as u8,
                    y1: y,
                };
                if best.as_ref().is_none_or(|b| candidate.better_than(b)) {
                    best = Some(candidate);
                }
                start = s;
            }
            if h > 0 {
                stack.push((start, h));
            }
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_round_trip(bits: &BitMatrix, mesh: &RectMesh) {
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                assert_eq!(
                    bits.get(x, y),
                    mesh.get(x, y),
                    "mismatch at ({x}, {y})"
                );
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

    fn assert_no_overlaps(mesh: &RectMesh) {
        for a in 0..mesh.rects().len() {
            for b in (a + 1)..mesh.rects().len() {
                let (ra, rb) = (mesh.rects()[a], mesh.rects()[b]);
                let overlaps = ra.x0 <= rb.x1
                    && rb.x0 <= ra.x1
                    && ra.y0 <= rb.y1
                    && rb.y0 <= ra.y1;
                assert!(!overlaps, "rects {a} and {b} overlap: {ra:?} {rb:?}");
            }
        }
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

    /// The worst case for this greedy algorithm: no two set cells share
    /// an edge, so every rectangle is 1x1 and each of the O(rects) passes
    /// still scans the whole grid. Confirms correctness holds even here;
    /// not run by default since it takes on the order of ten seconds.
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
