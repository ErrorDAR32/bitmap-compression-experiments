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
//! Everything per tile is held in [`pyramids`]. The grammar and what
//! each bit costs are in `docs/gct.md`.

pub mod complex_tiler;
pub mod decode;
pub mod encode;
pub mod grammar;
pub mod greedy_tiler;
pub mod nested_resolutions;
pub mod pyramids;
pub mod tile;
pub mod tree_representation;

use crate::Bitmap;
use grammar::bit_stream::BitStream;
use pyramids::content::Content;
use pyramids::pyramid::Pyramid;

pub use decode::decode;

/// The tree the greedy complex tiler makes of `bitmap`: the bitmap's
/// content pyramid, built once; the greedy tiler's placements; the
/// complex tiler's complex tiling; and the tree read off it.
pub fn tree(bitmap: &Bitmap) -> Pyramid {
    let placements = greedy_tiler::greedy_tiler(&Pyramid::content(bitmap));
    let complex_tiling = complex_tiler::complex_tiler::complex_tiler(placements);
    tree_representation::tree_representation(&complex_tiling)
}

/// Encodes `bitmap`.
pub fn encode(bitmap: &Bitmap) -> BitStream {
    encode::write(&tree(bitmap), bitmap)
}
