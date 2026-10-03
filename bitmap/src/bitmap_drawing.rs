//! Drawing on a bitmap: rectangles and circles, by their shape rather
//! than their cells.
//!
//! These take `i64` and clamp, so a caller can ask for a circle hanging
//! off the edge without doing the arithmetic first. They are things done
//! *to* a bitmap, where [`bitmap_data`](super::bitmap_data) holds what a
//! bitmap *is* and what can be asked of it.

use crate::{Bitmap, HEIGHT, WIDTH};

/// `x` pulled onto the bitmap, a column.
fn clamped_column(x: i64) -> u8 {
    x.clamp(0, WIDTH as i64 - 1) as u8
}

/// `y` pulled onto the bitmap, a row.
fn clamped_row(y: i64) -> u8 {
    y.clamp(0, HEIGHT as i64 - 1) as u8
}

impl Bitmap {
    /// Sets every cell of the inclusive rectangle with the two given
    /// opposite corners, named either way round, clamped to the bitmap.
    pub fn set_rect(&mut self, first_x: i64, first_y: i64, second_x: i64, second_y: i64) {
        self.for_each_in_rect(first_x, first_y, second_x, second_y, |bitmap, x, y| bitmap.set(x, y));
    }

    /// Clears every cell of the inclusive rectangle with the two given
    /// opposite corners, named either way round, clamped to the bitmap.
    pub fn unset_rect(&mut self, first_x: i64, first_y: i64, second_x: i64, second_y: i64) {
        self.for_each_in_rect(first_x, first_y, second_x, second_y, |bitmap, x, y| bitmap.unset(x, y));
    }

    /// Visits every cell of a rectangle given by two opposite corners in
    /// any order, clamped to the bitmap: shared by setting and clearing,
    /// so the two cannot disagree about what a rectangle is.
    fn for_each_in_rect(
        &mut self,
        first_x: i64,
        first_y: i64,
        second_x: i64,
        second_y: i64,
        mut visit: impl FnMut(&mut Self, u8, u8),
    ) {
        let (left, right) = (clamped_column(first_x.min(second_x)), clamped_column(first_x.max(second_x)));
        let (top, bottom) = (clamped_row(first_y.min(second_y)), clamped_row(first_y.max(second_y)));
        for y in top..=bottom {
            for x in left..=right {
                visit(self, x, y);
            }
        }
    }

    /// Sets every cell whose centre lies within `radius` of
    /// `(centre_x, centre_y)`.
    pub fn set_circle(&mut self, centre_x: i64, centre_y: i64, radius: i64) {
        self.for_each_in_circle(centre_x, centre_y, radius, |bitmap, x, y| bitmap.set(x, y));
    }

    /// Clears every cell whose centre lies within `radius` of
    /// `(centre_x, centre_y)`.
    pub fn unset_circle(&mut self, centre_x: i64, centre_y: i64, radius: i64) {
        self.for_each_in_circle(centre_x, centre_y, radius, |bitmap, x, y| bitmap.unset(x, y));
    }

    /// Visits every cell whose centre lies within `radius` of
    /// `(centre_x, centre_y)`, by walking the bounding box and testing
    /// squared distance, so no square root is taken. A negative radius
    /// draws nothing.
    fn for_each_in_circle(&mut self, centre_x: i64, centre_y: i64, radius: i64, mut visit: impl FnMut(&mut Self, u8, u8)) {
        if radius < 0 {
            return;
        }
        let radius_squared = radius * radius;
        let (left, right) = (clamped_column(centre_x - radius), clamped_column(centre_x + radius));
        let (top, bottom) = (clamped_row(centre_y - radius), clamped_row(centre_y + radius));
        for y in top..=bottom {
            let dy = y as i64 - centre_y;
            for x in left..=right {
                let dx = x as i64 - centre_x;
                if dx * dx + dy * dy <= radius_squared {
                    visit(self, x, y);
                }
            }
        }
    }
}
