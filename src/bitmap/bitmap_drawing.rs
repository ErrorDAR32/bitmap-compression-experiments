//! Drawing on a bitmap: rectangles and circles, by their shape rather
//! than their cells.
//!
//! These take `i64` and clamp, so a caller can ask for a circle
//! hanging off the edge without doing the arithmetic first. They are
//! here rather than beside the bits because they are things done *to*
//! a bitmap, where [`bitmap_data`](super::bitmap_data) holds what a
//! bitmap *is* and what can be asked of it.

use crate::{Bitmap, HEIGHT, WIDTH};

impl Bitmap {
    /// Sets every bit contained in the inclusive rectangle described by
    /// the two corner points, in any order. Coordinates are clamped to
    /// the matrix bounds.
    pub fn set_rect(&mut self, x0: i64, y0: i64, x1: i64, y1: i64) {
        self.for_each_in_rect(x0, y0, x1, y1, |m, x, y| m.set(x, y));
    }

    /// Unsets every bit contained in the inclusive rectangle described by
    /// the two corner points, in any order. Coordinates are clamped to
    /// the matrix bounds.
    pub fn unset_rect(&mut self, x0: i64, y0: i64, x1: i64, y1: i64) {
        self.for_each_in_rect(x0, y0, x1, y1, |m, x, y| m.unset(x, y));
    }

    /// Visits every cell of a rectangle given in any order and in
    /// coordinates that may lie outside the matrix, which is what lets
    /// the drawing methods take `i64` and clamp. Shared by setting and
    /// unsetting so the two cannot disagree about what a rectangle is.
    fn for_each_in_rect(
        &mut self,
        x0: i64,
        y0: i64,
        x1: i64,
        y1: i64,
        op: impl Fn(&mut Self, u8, u8),
    ) {
        let (lo_x, hi_x) = order(x0, x1);
        let (lo_y, hi_y) = order(y0, y1);
        let lo_x = clamp(lo_x, 0, WIDTH as i64 - 1) as u8;
        let hi_x = clamp(hi_x, 0, WIDTH as i64 - 1) as u8;
        let lo_y = clamp(lo_y, 0, HEIGHT as i64 - 1) as u8;
        let hi_y = clamp(hi_y, 0, HEIGHT as i64 - 1) as u8;

        for y in lo_y..=hi_y {
            for x in lo_x..=hi_x {
                op(self, x, y);
            }
        }
    }

    /// Sets every bit whose center lies within `radius` of `(cx, cy)`,
    /// approximating a filled circle by testing squared distance.
    pub fn set_circle(&mut self, cx: i64, cy: i64, radius: i64) {
        self.for_each_in_circle(cx, cy, radius, |m, x, y| m.set(x, y));
    }

    /// Unsets every bit whose center lies within `radius` of `(cx, cy)`.
    pub fn unset_circle(&mut self, cx: i64, cy: i64, radius: i64) {
        self.for_each_in_circle(cx, cy, radius, |m, x, y| m.unset(x, y));
    }

    /// Visits every cell whose centre lies within `radius` of
    /// `(cx, cy)`, by walking the bounding box and testing squared
    /// distance, so no square root is taken and nothing is approximated
    /// beyond the pixel grid itself. A negative radius draws nothing.
    fn for_each_in_circle(
        &mut self,
        cx: i64,
        cy: i64,
        radius: i64,
        op: impl Fn(&mut Self, u8, u8),
    ) {
        if radius < 0 {
            return;
        }
        let r2 = radius * radius;
        let lo_x = clamp(cx - radius, 0, WIDTH as i64 - 1) as u8;
        let hi_x = clamp(cx + radius, 0, WIDTH as i64 - 1) as u8;
        let lo_y = clamp(cy - radius, 0, HEIGHT as i64 - 1) as u8;
        let hi_y = clamp(cy + radius, 0, HEIGHT as i64 - 1) as u8;

        for y in lo_y..=hi_y {
            let dy = y as i64 - cy;
            for x in lo_x..=hi_x {
                let dx = x as i64 - cx;
                if dx * dx + dy * dy <= r2 {
                    op(self, x, y);
                }
            }
        }
    }

}

/// The two given either way round, smaller first, so that a caller can
/// name a rectangle by any two opposite corners.
fn order(a: i64, b: i64) -> (i64, i64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// `v` pulled into `lo..=hi`. Written out rather than `i64::clamp` so
/// that the intent is visible next to the coordinate arithmetic it
/// serves.
fn clamp(v: i64, lo: i64, hi: i64) -> i64 {
    v.max(lo).min(hi)
}
