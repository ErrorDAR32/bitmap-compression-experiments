//! gct, the greedy complex tiler: an encoding of a bitmap in four
//! steps, each its own folder or file, each reading only the one before
//! it --
//!
//! 1. [`greedy_tiler`]: tiles placed biggest first, each bound to one
//!    value or copying a same-size area -- a placements pyramid.
//! 2. [`complex_tiler`]: those tiles grouped into complex tiles, nested
//!    as deep as they keep paying -- a complex tile size offsets pyramid.
//! 3. [`tree_representation`]: the tree read off both, one node per
//!    tile -- a [tree pyramid](pyramids::tree) of node codes.
//! 4. [`encoder`]: that tree spelled out in bits, and read back;
//!    [`decode`](fn@decode) then resolves copies into cells.
//!
//! Everything per tile is held in [`pyramids`]. The grammar and what
//! each bit costs are in `docs/gct.md`.

pub mod complex_tiler;
pub mod decode;
pub mod encoder;
pub mod greedy_tiler;
pub mod nested_resolutions;
pub mod pyramids;
pub mod tile;
pub mod tree_representation;

use crate::Bitmap;
use encoder::bit_stream::BitStream;
use pyramids::bound_tile_counts::BoundTileCounts;
use pyramids::copyable::Copyable;
use pyramids::homogeneity::Homogeneity;
use pyramids::pyramid::Pyramid;
use tree_representation::ComplexTiles;

pub use decode::decode;

/// The tree the greedy complex tiler makes of `bitmap`: the bitmap's
/// homogeneity and copyable pyramids, built once; the greedy tiler's
/// placements; the complex tiler's size offsets over them; and the tree read
/// off both.
pub fn tree(bitmap: &Bitmap) -> Pyramid {
    let homogeneity = Pyramid::homogeneity(bitmap);
    let copyable = Pyramid::copyable(bitmap);
    let placements = greedy_tiler::greedy_tiler(bitmap, &homogeneity, &copyable);
    let bound_tile_counts = Vec::<Pyramid>::bound_tile_counts(&placements);
    let size_offsets = complex_tiler::complex_tiler::complex_tiler(&placements, &bound_tile_counts);
    tree_representation::tree_representation(&ComplexTiles { placements: &placements, bound_tile_counts: &bound_tile_counts, size_offsets: &size_offsets })
}

/// Encodes `bitmap`.
pub fn encode(bitmap: &Bitmap) -> BitStream {
    encoder::write(&tree(bitmap), bitmap)
}
