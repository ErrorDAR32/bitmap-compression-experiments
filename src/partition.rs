//! What both algorithms are: something you hand a bitmap and get
//! rectangles back from.
//!
//! The two have nothing in common inside. One reduces the bitmap to
//! runs and rewrites what it meshes; the other finds reflex corners,
//! matches chords and cuts. They have the same shape from outside, and
//! this is that shape, so that a benchmark, a test or a caller can hold
//! either without knowing which.
//!
//! Both are workspaces rather than free functions. Partitioning a
//! bitmap needs a few hundred kilobytes of room, and a caller with
//! layers to get through wants that found once rather than once a
//! bitmap; see the throughput note on [`crate`].

use crate::{BitMatrix, Rect};

/// Splits a bitmap's set bits into disjoint rectangles.
///
/// The rectangles cover every set bit exactly once, and the slice
/// belongs to the workspace until the next bitmap goes through it.
pub trait Partition {
    /// What to call this in a report.
    fn name(&self) -> &'static str;

    /// The rectangles, for the bitmap given.
    fn partition(&mut self, bits: &BitMatrix) -> &[Rect];
}

/// Panics unless the rectangles cover exactly the set bits, once each.
///
/// Overlap falls out of arithmetic rather than comparing every pair: if
/// the areas sum to more than the cells painted, two rectangles covered
/// the same cell.
pub fn assert_partition(bits: &BitMatrix, rects: &[Rect], label: &str) {
    let mut painted = BitMatrix::new();
    for r in rects {
        painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
    }
    assert_eq!(painted.count_set(), bits.count_set(), "{label}: wrong coverage");
    let area: u32 = rects.iter().map(|r| r.area()).sum();
    assert_eq!(area, painted.count_set(), "{label}: rectangles overlap");
}
