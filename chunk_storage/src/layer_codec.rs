//! What a layer is, and how its bitmap is encoded and decoded.
//!
//! A layer is a pair of a [`LayerType`] -- what it represents, from
//! specific things to properties -- and a bitmap marking the cells where
//! it holds, Tessera-encoded in 64-bit words. A Tessera stream ends
//! itself, so an encoded bitmap's exact length is never kept: decoding
//! reads from its first word, whatever follows its last.
//!
//! [`LayerCodec`] holds what encoding and decoding need, allocated once:
//! a Tessera, its stream, and the bitmap it decodes into. Encoding reads
//! a bitmap's cells from wherever they are held -- an arena's bucket,
//! say -- and decoding writes them there.

use bitmap::{Bitmap, CellWords};
use tessera::{BitStream, Tessera, MOST_BITS};

/// The most words an encoded bitmap takes.
const MOST_WORDS: usize = MOST_BITS.div_ceil(u64::BITS as usize);

/// What a layer represents: a `u64` naming anything from a specific
/// thing to a property. What each value means is not this crate's
/// business; only that two layers of a chunk never share one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerType(pub u64);

/// Encodes and decodes layers' bitmaps, holding everything either
/// needs, allocated once and reused for every bitmap.
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

    /// `cells`, encoded: its words, the stream's bits rounded up to the
    /// next word, until the next encoding.
    pub fn encode(&mut self, cells: &CellWords) -> &[u64] {
        self.bitmap.words_mut().copy_from_slice(cells);
        self.tessera.encode(&self.bitmap, &mut self.stream);
        self.stream.words()
    }

    /// Decodes the bitmap whose encoding starts at `words` into `cells`,
    /// whatever they held before. What follows the encoding in `words`
    /// -- other bitmaps, say -- is read past, not decoded.
    pub fn decode(&mut self, words: &[u64], cells: &mut CellWords) {
        self.stream.load_words(&words[..words.len().min(MOST_WORDS)]);
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
