//! Which tiles the complex tiler made complex tiles, and at what depth:
//! four bits a tile, `0` for none, otherwise the depth (the resolution
//! is that many levels finer than the tile).
//!
//! Only tiles of 4x4 and coarser can be complex tiles: a resolution is
//! never finer than 2x2.

use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{Tile, CELL_LEVEL};

const NONE: u64 = 0;

const SHAPE: PyramidShape = PyramidShape { arity: 4, coarsest_level: 0, finest_level: CELL_LEVEL - 2, element_bits: 4 };

pub trait ComplexTileDepths {
    /// No complex tiles anywhere yet.
    fn complex_tile_depths() -> Self;

    /// The depth of the complex tile at exactly `tile`, if it is one.
    fn complex_tile_depth(&self, tile: Tile) -> Option<usize>;

    fn set_complex_tile_depth(&mut self, tile: Tile, depth: usize);
}

impl ComplexTileDepths for Pyramid {
    fn complex_tile_depths() -> Self {
        Pyramid::new(SHAPE)
    }

    fn complex_tile_depth(&self, tile: Tile) -> Option<usize> {
        if !self.holds(tile) {
            return None;
        }
        let depth = self.get(tile);
        (depth != NONE).then_some(depth as usize)
    }

    fn set_complex_tile_depth(&mut self, tile: Tile, depth: usize) {
        assert!(depth >= 1, "a complex tile's resolution is finer than itself");
        self.set(tile, depth as u64);
    }
}
