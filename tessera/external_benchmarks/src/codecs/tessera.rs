//! Tessera itself, one [`tessera::Tessera`] kept for every bitmap, as a
//! caller encoding many bitmaps would use it.

use super::Codec;
use crate::rows::{Rows, HEIGHT, WIDTH};
use tessera::grammar::bit_stream::BitStream;
use tessera::Bitmap;

/// Tessera, with its stream and decoded bitmap kept between bitmaps.
pub struct Tessera {
    /// Every structure encoding and decoding need.
    tessera: tessera::Tessera,
    /// The last stream written.
    stream: BitStream,
    /// The last bitmap decoded.
    back: Bitmap,
}

impl Tessera {
    /// Everything allocated once.
    pub fn new() -> Self {
        Self { tessera: tessera::Tessera::new(), stream: BitStream::default(), back: Bitmap::new() }
    }
}

impl Default for Tessera {
    /// The same as [`Tessera::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl Codec for Tessera {
    fn name(&self) -> String {
        "Tessera".to_string()
    }

    fn encode(&mut self, bitmap: &Bitmap, _: &Rows) {
        self.tessera.encode(bitmap, &mut self.stream);
    }

    fn encoded_bits(&self) -> usize {
        self.stream.len()
    }

    fn decode(&mut self) {
        self.tessera.decode(&self.stream, &mut self.back);
    }

    fn decoded_matches(&self, rows: &Rows) -> bool {
        (0..HEIGHT).all(|y| (0..WIDTH).all(|x| self.back.get(x as u8, y as u8) == rows.get(x, y)))
    }
}
