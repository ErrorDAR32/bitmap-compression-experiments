//! gct itself, one [`tilesim::gct::Gct`] kept for every bitmap, as a
//! caller encoding many bitmaps would use it.

use super::Codec;
use crate::rows::{Rows, HEIGHT, WIDTH};
use tilesim::gct::grammar::bit_stream::BitStream;
use tilesim::Bitmap;

/// gct, with its stream and decoded bitmap kept between bitmaps.
pub struct Gct {
    /// Every structure encoding and decoding need.
    gct: tilesim::gct::Gct,
    /// The last stream written.
    stream: BitStream,
    /// The last bitmap decoded.
    back: Bitmap,
}

impl Gct {
    /// Everything allocated once.
    pub fn new() -> Self {
        Self { gct: tilesim::gct::Gct::new(), stream: BitStream::default(), back: Bitmap::new() }
    }
}

impl Default for Gct {
    /// The same as [`Gct::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl Codec for Gct {
    fn name(&self) -> String {
        "gct".to_string()
    }

    fn encode(&mut self, bitmap: &Bitmap, _: &Rows) {
        self.gct.encode(bitmap, &mut self.stream);
    }

    fn encoded_bits(&self) -> usize {
        self.stream.len()
    }

    fn decode(&mut self) {
        self.gct.decode(&self.stream, &mut self.back);
    }

    fn decoded_matches(&self, rows: &Rows) -> bool {
        (0..HEIGHT).all(|y| (0..WIDTH).all(|x| self.back.get(x as u8, y as u8) == rows.get(x, y)))
    }
}
