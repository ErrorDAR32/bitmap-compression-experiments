//! A layer as a disk chunk holds it in memory: its bitmap
//! Tessera-encoded, in 64-bit words, aligned -- a sparse layer takes a
//! word or a few, not a bitmap's 8 KiB -- and decoded only when a cell
//! of it is needed.
//!
//! Its length is its words, the stream's bits rounded up to the next
//! word: the last word's spare bits are 0, and decoding reads them as
//! the 0s past a stream's end, so the exact bit count is not kept. On
//! disk a layer will be packed to the byte instead
//! (`BitStream::to_bytes`).
//!
//! [`LayerCodec`] holds what encoding and decoding need, allocated once:
//! a Tessera, its stream, and the bitmap it decodes into. Encoding reads
//! a bitmap's cells from wherever they are held -- an arena's bucket,
//! say -- and decoding writes them there.

use bitmap::{Bitmap, CellWords};
use tessera::BitStream;
use tessera::Tessera;

/// A bitmap, Tessera-encoded, in aligned 64-bit words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedLayer {
    /// The stream's bits, the first lowest in the first word.
    words: Box<[u64]>,
}

impl EncodedLayer {
    /// How many words the encoding takes.
    pub fn word_len(&self) -> usize {
        self.words.len()
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
        EncodedLayer { words: self.stream.words().into() }
    }

    /// Decodes `layer` into `cells`, whatever they held before.
    pub fn decode(&mut self, layer: &EncodedLayer, cells: &mut CellWords) {
        self.stream.load_words(&layer.words);
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
