//! A layer as a disk chunk holds it: its bitmap Tessera-encoded, kept
//! at its exact length -- a sparse layer takes a few words, not a
//! bitmap's 1024 -- and decoded only when a cell of it is needed.
//!
//! [`LayerCodec`] holds what encoding and decoding need, allocated once:
//! a Tessera, its stream, and the bitmap it decodes into. Encoding reads
//! a bitmap's cells from wherever they are held -- an arena's bucket,
//! say -- and decoding writes them there.

use bitmap::{Bitmap, CellWords};
use tessera::grammar::bit_stream::BitStream;
use tessera::Tessera;

/// A bitmap, Tessera-encoded, at its exact length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedLayer {
    /// How many bits the stream is.
    bits: usize,
    /// The stream's words, as many as its bits take.
    words: Box<[u64]>,
}

impl EncodedLayer {
    /// How many bits the encoding takes.
    pub fn bits(&self) -> usize {
        self.bits
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
        EncodedLayer { bits: self.stream.len(), words: self.stream.words().into() }
    }

    /// Decodes `layer` into `cells`, whatever they held before.
    pub fn decode(&mut self, layer: &EncodedLayer, cells: &mut CellWords) {
        self.stream.load(&layer.words, layer.bits);
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
