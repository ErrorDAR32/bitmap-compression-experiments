//! A generic pyramid: one element per tile, at every level between a
//! coarsest and a finest, each element a fixed number of bits, packed
//! into machine words.
//!
//! A tile is its level and its (x, y) in that level's plane; each
//! level's plane is stored row by row. A tile's children are the
//! `arity` tiles one level finer that fill it -- with `arity` 4, the
//! 2x2 block at (2x..2x+1, 2y..2y+1).
//!
//! A specialized pyramid fixes the four parameters and supplies its
//! own queries and actions (see the other files in this folder). An
//! action is what [`Pyramid::propagate`] uses to work out a tile's
//! element from its children's: set the finest level, then propagate
//! once, and every coarser level follows.

use crate::gct::tile::Tile;

/// The four parameters every pyramid is built from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PyramidShape {
    /// Children per tile. A perfect square: that many tiles, in a
    /// square block, fill their parent.
    pub arity: usize,
    /// The coarsest level held.
    pub coarsest_level: usize,
    /// The finest level held.
    pub finest_level: usize,
    /// Bits per element. Divides 64, so an element never straddles two
    /// words.
    pub element_bits: usize,
}

impl PyramidShape {
    /// Children along one side of a tile.
    fn children_across(self) -> usize {
        let across = self.arity.isqrt();
        assert_eq!(across * across, self.arity, "a pyramid's arity is a perfect square");
        across
    }

    /// Tiles across one row of `level`'s plane.
    fn tiles_across(self, level: usize) -> usize {
        self.children_across().pow(level as u32)
    }
}

/// One element per tile, per level -- see the module doc.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Pyramid {
    shape: PyramidShape,
    /// One word-packed plane per level, coarsest first.
    levels: Vec<Vec<u64>>,
}

impl Pyramid {
    /// An all-zero pyramid of this shape.
    pub fn new(shape: PyramidShape) -> Self {
        assert!(
            shape.element_bits > 0 && u64::BITS as usize % shape.element_bits == 0,
            "an element's bits divide a word"
        );
        assert!(shape.coarsest_level <= shape.finest_level);
        let per_word = u64::BITS as usize / shape.element_bits;
        let levels = (shape.coarsest_level..=shape.finest_level)
            .map(|level| {
                let across = shape.tiles_across(level);
                vec![0u64; (across * across).div_ceil(per_word)]
            })
            .collect();
        Self { shape, levels }
    }

    pub fn shape(&self) -> PyramidShape {
        self.shape
    }

    /// Whether this pyramid holds `tile`'s level at all.
    pub fn holds(&self, tile: Tile) -> bool {
        (self.shape.coarsest_level..=self.shape.finest_level).contains(&tile.level)
    }

    /// Where a tile's element sits: its level's plane, the word, and
    /// the shift within it.
    fn locate(&self, tile: Tile) -> (usize, usize, usize) {
        debug_assert!(self.holds(tile), "{tile:?} is outside this pyramid's levels");
        let index = tile.y * self.shape.tiles_across(tile.level) + tile.x;
        let per_word = u64::BITS as usize / self.shape.element_bits;
        (tile.level - self.shape.coarsest_level, index / per_word, (index % per_word) * self.shape.element_bits)
    }

    fn element_mask(&self) -> u64 {
        if self.shape.element_bits == u64::BITS as usize {
            u64::MAX
        } else {
            (1 << self.shape.element_bits) - 1
        }
    }

    /// A tile's element.
    pub fn get(&self, tile: Tile) -> u64 {
        let (plane, word, shift) = self.locate(tile);
        (self.levels[plane][word] >> shift) & self.element_mask()
    }

    /// Replaces a tile's element.
    pub fn set(&mut self, tile: Tile, value: u64) {
        let mask = self.element_mask();
        debug_assert!(value & !mask == 0, "{value} does not fit in {} bits", self.shape.element_bits);
        let (plane, word, shift) = self.locate(tile);
        let slot = &mut self.levels[plane][word];
        *slot = (*slot & !(mask << shift)) | (value << shift);
    }

    /// A tile's children, in reading order.
    pub fn children_of(&self, tile: Tile) -> Vec<Tile> {
        let across = self.shape.children_across();
        let mut out = Vec::with_capacity(self.shape.arity);
        for row in 0..across {
            for col in 0..across {
                out.push(Tile { level: tile.level + 1, x: tile.x * across + col, y: tile.y * across + row });
            }
        }
        out
    }

    /// Every tile of one level, in reading order.
    pub fn tiles_of_level(&self, level: usize) -> impl Iterator<Item = Tile> {
        let across = self.shape.tiles_across(level);
        (0..across).flat_map(move |y| (0..across).map(move |x| Tile { level, x, y }))
    }

    /// Recomputes every level coarser than the finest, finest first,
    /// each tile's element being `action` of its children's elements in
    /// reading order.
    pub fn propagate(&mut self, action: impl Fn(&[u64]) -> u64) {
        let mut children = Vec::with_capacity(self.shape.arity);
        for level in (self.shape.coarsest_level..self.shape.finest_level).rev() {
            let across = self.shape.tiles_across(level);
            for y in 0..across {
                for x in 0..across {
                    let tile = Tile { level, x, y };
                    children.clear();
                    children.extend(self.children_of(tile).into_iter().map(|child| self.get(child)));
                    let value = action(&children);
                    self.set(tile, value);
                }
            }
        }
    }
}
