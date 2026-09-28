//! cgt, the complex greedy tiler: an encoding of a bitmap in four
//! steps, each its own folder or file, each reading only the one before
//! it --
//!
//! 1. [`greedy_tiler`]: tiles placed biggest first, each bound to one
//!    value or copying a same-size area -- a placements pyramid.
//! 2. [`complex_tiler`]: those tiles grouped into complex tiles, nested
//!    as deep as they keep paying -- a complex tile depths pyramid.
//! 3. [`tree`]: the tree read off both, one node per tile -- a pyramid
//!    of node codes.
//! 4. [`encoder`]: that tree spelled out in bits, and read back;
//!    [`decode`] then resolves copies into cells.
//!
//! Everything per tile is held in [`pyramids`]. The grammar and what
//! each bit costs are in `docs/cgt.md`.

pub mod complex_tiler;
pub mod decode;
pub mod enclosing;
pub mod encoder;
pub mod greedy_tiler;
pub mod pyramids;
pub mod tile;
pub mod tree;
pub mod tree_stats;

use crate::Bitmap;
use encoder::bit_stream::BitStream;
use pyramids::bound_tile_counts::BoundTileCounts;
use pyramids::pyramid::Pyramid;
use tree::from_complex_tiles::ComplexTiles;

pub use decode::decode;

/// The tree the complex greedy tiler makes of `bitmap`: the greedy
/// tiler's placements, the complex tiler's depths over them, and the
/// tree read off both.
pub fn tree(bitmap: &Bitmap) -> Pyramid {
    let placements = greedy_tiler::greedy_tiler(bitmap);
    let counts = Vec::<Pyramid>::bound_tile_counts(&placements);
    let depths = complex_tiler::complex_tiler::complex_tiler(&placements, &counts);
    tree::from_complex_tiles::tree_from_complex_tiles(&ComplexTiles { placements: &placements, counts: &counts, depths: &depths })
}

/// Encodes `bitmap`.
pub fn encode(bitmap: &Bitmap) -> BitStream {
    encoder::write(&tree(bitmap), bitmap)
}
