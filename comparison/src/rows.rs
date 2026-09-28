//! A bitmap as the external codecs take it: rows of packed bits, top row
//! first, most significant bit leftmost, 1 a set cell -- the raster
//! layout G4 and JBIG work in.

pub use bitmap::{HEIGHT, WIDTH};
use bitmap::Bitmap;

/// Bytes a row.
pub const ROW_BYTES: usize = WIDTH / 8;
/// Bytes the whole bitmap takes.
pub const BYTES: usize = ROW_BYTES * HEIGHT;

/// A bitmap in rows of packed bits.
#[derive(Clone, PartialEq, Eq)]
pub struct Rows(pub Vec<u8>);

impl Rows {
    /// `bitmap`'s cells, a row at a time -- read one cell at a time, so
    /// done before anything is timed.
    pub fn of(bitmap: &Bitmap) -> Self {
        let mut bytes = vec![0; BYTES];
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                if bitmap.get(x as u8, y as u8) {
                    bytes[y * ROW_BYTES + x / 8] |= 0x80 >> (x % 8);
                }
            }
        }
        Rows(bytes)
    }

    /// Whether the cell at `(x, y)` is set.
    pub fn get(&self, x: usize, y: usize) -> bool {
        self.0[y * ROW_BYTES + x / 8] & (0x80 >> (x % 8)) != 0
    }
}
