//! Which tiles the complex tiler made complex tiles, and at what size
//! offset: four bits a tile, `0` for none, otherwise the size offset (the resolution
//! is that many levels finer than the tile).
//!
//! Only tiles of 4x4 and coarser can be complex tiles: a resolution is
//! never finer than 2x2.

use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{Tile, CELL_LEVEL};

const NONE: u64 = 0;

const SHAPE: PyramidShape = PyramidShape { arity: 4, coarsest_level: 0, finest_level: CELL_LEVEL - 2, element_bits: 4 };

pub trait ComplexTileSizeOffsets {
    /// No complex tiles anywhere yet.
    fn complex_tile_size_offsets() -> Self;

    /// The size offset of the complex tile at exactly `tile`, if it is one.
    fn complex_tile_size_offset(&self, tile: Tile) -> Option<usize>;

    fn set_complex_tile_size_offset(&mut self, tile: Tile, size_offset: usize);
}

impl ComplexTileSizeOffsets for Pyramid {
    fn complex_tile_size_offsets() -> Self {
        Pyramid::new(SHAPE)
    }

    fn complex_tile_size_offset(&self, tile: Tile) -> Option<usize> {
        if !self.holds(tile) {
            return None;
        }
        let size_offset = self.get(tile);
        (size_offset != NONE).then_some(size_offset as usize)
    }

    fn set_complex_tile_size_offset(&mut self, tile: Tile, size_offset: usize) {
        assert!(size_offset >= 1, "a complex tile's resolution is finer than itself");
        self.set(tile, size_offset as u64);
    }
}
