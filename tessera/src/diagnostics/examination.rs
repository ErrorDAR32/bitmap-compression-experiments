//! One bitmap encoded and decoded by a `Tessera`: the bits written, the
//! stream's mode, and the first cell decoding gets wrong, if any. That
//! the tree written is the tree counted, the encoder checks itself in
//! debug builds.

use crate::tree::Tree;
use crate::{BitStream, Tessera};
use bitmap::Bitmap;

/// What one bitmap's encoding and decoding came to.
#[derive(Clone, Debug)]
pub struct Examination {
    /// The bits written.
    pub written_bits: usize,
    /// Whether the stream is the binary count tree rather than the tree.
    pub binary_count_tree: bool,
    /// The first cell, in reading order, decoded wrong, if any.
    pub first_difference: Option<(u8, u8)>,
}

/// The first cell, in reading order, where two bitmaps differ.
pub fn first_difference(a: &Bitmap, b: &Bitmap) -> Option<(u8, u8)> {
    if a.words() == b.words() {
        return None;
    }
    (0..=u8::MAX).flat_map(|y| (0..=u8::MAX).map(move |x| (x, y))).find(|&(x, y)| a.get(x, y) != b.get(x, y))
}

/// The tree Tessera makes of `bitmap`, whichever stream it writes.
pub fn tree_of(bitmap: &Bitmap) -> Tree {
    let mut tessera = Tessera::new();
    tessera.encode(bitmap, &mut BitStream::default());
    tessera.tree().clone()
}

impl Examination {
    /// Encodes `bitmap` with `tessera` into `stream` and decodes it into
    /// `back`.
    pub fn of(tessera: &mut Tessera, stream: &mut BitStream, back: &mut Bitmap, bitmap: &Bitmap) -> Self {
        tessera.encode(bitmap, stream);
        tessera.decode(stream, back);
        Self { written_bits: stream.len(), binary_count_tree: stream.words()[0] & 1 == 1, first_difference: first_difference(bitmap, back) }
    }
}
