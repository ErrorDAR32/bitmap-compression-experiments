//! gct itself, through its workspace, as a caller encoding many bitmaps
//! would use it.

use super::Codec;
use crate::rows::{Rows, HEIGHT, WIDTH};
use tilesim::gct::grammar::bit_stream::BitStream;
use tilesim::gct::Workspace;
use tilesim::Bitmap;

/// gct, with its workspace, stream and decoded bitmap kept between
/// bitmaps.
pub struct Gct {
    /// Every structure encoding and decoding need.
    workspace: Workspace,
    /// The last stream written.
    stream: BitStream,
    /// The last bitmap decoded.
    back: Bitmap,
}

impl Gct {
    /// Everything allocated once.
    pub fn new() -> Self {
        Self { workspace: Workspace::new(), stream: BitStream::default(), back: Bitmap::new() }
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
        self.workspace.encode(bitmap, &mut self.stream);
    }

    fn encoded_bits(&self) -> usize {
        self.stream.len()
    }

    fn decode(&mut self) {
        self.workspace.decode(&self.stream, &mut self.back);
    }

    fn decoded_matches(&self, rows: &Rows) -> bool {
        (0..HEIGHT).all(|y| (0..WIDTH).all(|x| self.back.get(x as u8, y as u8) == rows.get(x, y)))
    }
}
