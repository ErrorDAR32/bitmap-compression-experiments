//! A layer as a disk chunk holds it: its bitmap Tessera-encoded, packed
//! to the byte -- a sparse layer takes a few bytes, not a bitmap's 8 KiB
//! -- and decoded only when a cell of it is needed.
//!
//! Its length is its bytes, the stream's bits rounded up to the next
//! byte: the last byte's spare bits are 0, and decoding reads them as the
//! 0s past a stream's end, so the exact bit count is not kept. Bytes
//! rather than 8-byte words: decoding copies a layer into the codec's
//! stream either way, so word alignment would buy nothing, and bytes
//! spare the padding and a separate length.
//!
//! [`LayerCodec`] holds what encoding and decoding need, allocated once:
//! a Tessera, its stream, and the bitmap it decodes into. Encoding reads
//! a bitmap's cells from wherever they are held -- an arena's bucket,
//! say -- and decoding writes them there.

use bitmap::{Bitmap, CellWords};
use tessera::grammar::bit_stream::BitStream;
use tessera::Tessera;

/// A bitmap, Tessera-encoded, packed to the byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedLayer {
    /// The stream's bits, the first lowest in the first byte.
    bytes: Box<[u8]>,
}

impl EncodedLayer {
    /// How many bytes the encoding takes.
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }
}

/// Encodes and decodes layers, holding everything either needs,
/// allocated once and reused for every layer.
pub struct LayerCodec {
    /// The encoding itself.
    tessera: Tessera,
    /// The stream encoding writes and decoding reads.
    stream: BitStream,
    /// The bitmap encoding reads and decoding writes.
    bitmap: Bitmap,
}

impl LayerCodec {
    /// Everything allocated.
    pub fn new() -> Self {
        Self { tessera: Tessera::new(), stream: BitStream::default(), bitmap: Bitmap::new() }
    }

    /// `cells`, encoded.
    pub fn encode(&mut self, cells: &CellWords) -> EncodedLayer {
        self.bitmap.words_mut().copy_from_slice(cells);
        self.tessera.encode(&self.bitmap, &mut self.stream);
        EncodedLayer { bytes: self.stream.to_bytes() }
    }

    /// Decodes `layer` into `cells`, whatever they held before.
    pub fn decode(&mut self, layer: &EncodedLayer, cells: &mut CellWords) {
        self.stream.load_bytes(&layer.bytes);
        self.tessera.decode(&self.stream, &mut self.bitmap);
        cells.copy_from_slice(self.bitmap.words());
    }
}

impl Default for LayerCodec {
    /// The same as [`LayerCodec::new`].
    fn default() -> Self {
        Self::new()
    }
}
