//! gct, the greedy complex tiler: an encoding of a bitmap in four
//! steps, each its own folder or file, each reading only the one before
//! it --
//!
//! 1. [`greedy_tiler`]: tiles placed biggest first, each bound to one
//!    value or copying a same-size area -- the placement bits of the
//!    complex tiling pyramid.
//! 2. [`complex_tiler`]: those tiles grouped into complex tiles, nested
//!    as deep as they keep paying -- a complex tiling pyramid.
//! 3. [`tree_representation`]: the tree read off it, one node per
//!    tile -- a [tree pyramid](pyramids::tree) of node codes.
//! 4. [`encode`](mod@encode): that tree spelled out in bits, by the
//!    [`grammar`]; [`decode`](mod@decode) reads it back by the same
//!    grammar and resolves copies into cells.
//!
//! Everything per tile is held in [`pyramids`], and every structure the
//! steps use lives in a [`Workspace`], allocated once and reused for
//! every bitmap. The grammar and what each bit costs are in
//! `docs/gct.md`.

pub mod complex_tiler;
pub mod decode;
pub mod encode;
pub mod grammar;
pub mod greedy_tiler;
pub mod nested_resolutions;
pub mod pyramids;
pub mod tile;
pub mod tree_representation;
pub mod workspace;

use crate::Bitmap;
use grammar::bit_stream::BitStream;

pub use workspace::Workspace;

/// Encodes `bitmap`, in a workspace of its own. To encode many, keep one
/// [`Workspace`] and a stream, and encode each into them.
pub fn encode(bitmap: &Bitmap) -> BitStream {
    let mut stream = BitStream::default();
    Workspace::new().encode(bitmap, &mut stream);
    stream
}

/// Decodes `stream`, in a workspace of its own. To decode many, keep one
/// [`Workspace`] and a bitmap, and decode each into them.
pub fn decode(stream: &BitStream) -> Bitmap {
    let mut bitmap = Bitmap::new();
    Workspace::new().decode(stream, &mut bitmap);
    bitmap
}
