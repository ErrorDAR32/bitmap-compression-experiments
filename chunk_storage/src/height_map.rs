//! A chunk's heights: one [`Height`] a cell, held raw. It will need
//! compressing on disk, as the layers are; in memory it stays raw.
//!
//! The heights are laid out in the Morton order the chunk's bitmaps
//! use (`bitmap::morton`), so any aligned square of cells is one run
//! of heights, as it is one run of bits in a layer: a later encoding
//! can read heights by the same tiles it reads the layers by.

use crate::coordinates::CellPlace;
use bitmap::morton::morton_index;
use bitmap::{HEIGHT, WIDTH};

/// A cell's height.
pub type Height = u8;

/// Cells in a chunk.
const CELLS: usize = WIDTH * HEIGHT;

/// A chunk's heights, one a cell, in Morton order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeightMap {
    /// Every cell's height, at the cell's Morton index.
    heights: Box<[Height; CELLS]>,
}

impl HeightMap {
    /// Every cell at `height`.
    pub fn filled(height: Height) -> Self {
        // Made on the heap: 64 KiB is too much to build on the stack
        // first.
        let heights = vec![height; CELLS].into_boxed_slice().try_into().expect("exactly a chunk's cells");
        Self { heights }
    }

    /// The height at `cell`.
    pub fn get(&self, cell: CellPlace) -> Height {
        self.heights[morton_index(cell.x, cell.y)]
    }

    /// Sets the height at `cell`.
    pub fn set(&mut self, cell: CellPlace, height: Height) {
        self.heights[morton_index(cell.x, cell.y)] = height;
    }

    /// Sets every cell to `height`.
    pub fn fill(&mut self, height: Height) {
        self.heights.fill(height);
    }

    /// Every height, in Morton order: what an encoding reads.
    pub fn in_morton_order(&self) -> &[Height; CELLS] {
        &self.heights
    }
}

impl Default for HeightMap {
    /// Every cell at height 0.
    fn default() -> Self {
        Self::filled(0)
    }
}
