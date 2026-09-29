//! The copy sources pyramid: for every 4x4 block, the block its cells
//! are copied from, while decoding resolves copies
//! ([`crate::gct::decode`](mod@crate::gct::decode)). One level, 16 bits a block: the source
//! block's Morton index plus one, 0 for a block no copy covers or one
//! already copied.

use super::copyable::FINEST_COPY_LEVEL;
use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{tiles_across, Tile};
use crate::morton::{morton_coordinates, morton_index};

/// The level copies are resolved at: 4x4 blocks. A copy is 4x4 or
/// coarser, and so is every child a masking copy says itself, so a
/// copy's own cells are always whole blocks.
pub const BLOCK_LEVEL: u8 = FINEST_COPY_LEVEL;
/// Blocks in the bitmap.
pub const BLOCKS: usize = tiles_across(BLOCK_LEVEL) * tiles_across(BLOCK_LEVEL);

/// One level, the blocks', 16 bits a block.
const SHAPE: PyramidShape = PyramidShape { coarsest_level: BLOCK_LEVEL, finest_level: BLOCK_LEVEL, element_bits: 16 };
const _: () = assert!(BLOCKS < 1 << SHAPE.element_bits);
/// A block no copy covers, or one already copied.
const NO_SOURCE: u64 = 0;

/// The copy sources pyramid's queries and updates.
pub trait CopySources {
    /// No block covered.
    fn copy_sources() -> Self;

    /// Notes that `block` is copied from `source`.
    fn set_source(&mut self, block: Tile, source: Tile);

    /// Where `block` is still to be copied from, if anywhere.
    fn source_of(&self, block: Tile) -> Option<Tile>;

    /// Notes that `block` has been copied.
    fn mark_copied(&mut self, block: Tile);
}

impl CopySources for Pyramid {
    fn copy_sources() -> Self {
        Pyramid::new(SHAPE)
    }

    fn set_source(&mut self, block: Tile, source: Tile) {
        self.set(block, morton_index(source.x, source.y) as u64 + 1);
    }

    #[inline]
    fn source_of(&self, block: Tile) -> Option<Tile> {
        let source = self.get(block);
        (source != NO_SOURCE).then(|| {
            let (x, y) = morton_coordinates(source as usize - 1);
            Tile { level: BLOCK_LEVEL, x, y }
        })
    }

    fn mark_copied(&mut self, block: Tile) {
        self.set(block, NO_SOURCE);
    }
}
