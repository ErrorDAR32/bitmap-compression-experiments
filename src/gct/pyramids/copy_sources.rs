//! The copy sources pyramid: for every 4x4 block, the block its cells
//! are copied from, while decoding resolves copies
//! ([`crate::gct::decode`](mod@crate::gct::decode)). One level, 16 bits a block: the source
//! block's Morton index plus one, 0 for a block no copy covers or one
//! already copied.

use super::copyable::FINEST_COPY_LEVEL;
use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{tiles_in_level, Tile};
use crate::morton::{morton_coordinates, morton_index};

/// The level copies are resolved at: 4x4 blocks. A copy is 4x4 or
/// coarser, and so is every child a masking copy says itself, so a
/// copy's own cells are always whole blocks.
pub const BLOCK_LEVEL: u8 = FINEST_COPY_LEVEL;
/// Blocks in the bitmap.
pub const BLOCKS: usize = tiles_in_level(BLOCK_LEVEL);

/// A block no copy covers, or one already copied.
const NO_SOURCE: u64 = 0;

/// One level, the blocks', 16 bits a block: a source's Morton index plus
/// one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CopySourcesShape;

impl PyramidShape for CopySourcesShape {
    const COARSEST_LEVEL: u8 = BLOCK_LEVEL;
    const FINEST_LEVEL: u8 = BLOCK_LEVEL;
    const ELEMENT_BITS: usize = u16::BITS as usize;
}
const _: () = assert!(BLOCKS < 1 << CopySourcesShape::ELEMENT_BITS);

/// The copy sources pyramid: each block's source, while decoding.
pub type CopySources = Pyramid<CopySourcesShape, { CopySourcesShape::WORDS }>;

impl CopySources {
    /// Notes that `block` is copied from `source`.
    pub fn set_source(&mut self, block: Tile, source: Tile) {
        self.set(block, morton_index(source.x, source.y) as u64 + 1);
    }

    /// Where `block` is still to be copied from, if anywhere.
    #[inline]
    pub fn source_of(&self, block: Tile) -> Option<Tile> {
        let source = self.get(block);
        (source != NO_SOURCE).then(|| {
            let (x, y) = morton_coordinates(source as usize - 1);
            Tile { level: BLOCK_LEVEL, x, y }
        })
    }

    /// Notes that `block` has been copied.
    pub fn mark_copied(&mut self, block: Tile) {
        self.set(block, NO_SOURCE);
    }
}
