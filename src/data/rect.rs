//! The one shape the whole crate deals in.

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

    /// One size up for the same reason [`Rect::width`] is.
    pub fn height(&self) -> u16 {
        self.y1 as u16 - self.y0 as u16 + 1
    }

    /// How many cells the rectangle covers. A `u32`, because a
    /// rectangle filling the matrix covers 65536 cells.
    pub fn area(&self) -> u32 {
        self.width() as u32 * self.height() as u32
    }
}
