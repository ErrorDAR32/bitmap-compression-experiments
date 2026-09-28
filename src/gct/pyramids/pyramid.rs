//! A generic pyramid: one element per tile, at every level between a
//! coarsest and a finest, each element a fixed number of bits, packed
//! into machine words.
//!
//! A tile is its level and its (x, y) in that level's plane; each
//! level's plane is stored row by row. A tile's children are the
//! `arity` tiles one level finer that fill it -- with `arity` 4, the
//! 2x2 block at (2x..2x+1, 2y..2y+1).
//!
//! A specialized pyramid (the other files in this folder) fixes the
//! four parameters, supplies its own queries, and may give the pyramid a
//! [`Propagation`]: the rule for what a tile holds, given its children.
//! Then every [`Pyramid::set`] keeps the coarser levels in step on its
//! own: it recomputes the set tile's parent, then that one's parent, and
//! stops at the first whose element does not change -- often right
//! away, sometimes only at the whole bitmap.

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

/// What `tile` should hold, worked out from its children's elements in
/// `pyramid`.
pub type Propagation = fn(pyramid: &Pyramid, tile: Tile) -> u64;

/// One element per tile, per level -- see the module doc.
#[derive(Clone, Debug)]
pub struct Pyramid {
    shape: PyramidShape,
    propagation: Option<Propagation>,
    /// One word-packed plane per level, coarsest first.
    levels: Vec<Vec<u64>>,
}

/// Two pyramids are equal when they hold the same elements in the same
/// shape; how they propagate is behaviour, not content.
impl PartialEq for Pyramid {
    fn eq(&self, other: &Self) -> bool {
        self.shape == other.shape && self.levels == other.levels
    }
}

impl Eq for Pyramid {}

impl Pyramid {
    /// An all-zero pyramid of this shape, whose levels are independent:
    /// setting a tile changes nothing else.
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
        Self { shape, propagation: None, levels }
    }

    /// An all-zero pyramid of this shape that keeps its coarser levels
    /// in step with `propagation`. All zeros must already be in step:
    /// `propagation` of all-zero children is zero.
    pub fn with_propagation(shape: PyramidShape, propagation: Propagation) -> Self {
        Self { propagation: Some(propagation), ..Self::new(shape) }
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

    /// Replaces a tile's element, then propagates: each coarser tile
    /// holding it is recomputed, up to the first that does not change.
    pub fn set(&mut self, tile: Tile, value: u64) {
        self.write(tile, value);
        let Some(propagation) = self.propagation else { return };
        let mut changed = tile;
        while changed.level > self.shape.coarsest_level {
            let parent = self.parent_of(changed);
            let value = propagation(self, parent);
            if value == self.get(parent) {
                return;
            }
            self.write(parent, value);
            changed = parent;
        }
    }

    /// Replaces one tile's element, nothing else.
    fn write(&mut self, tile: Tile, value: u64) {
        let mask = self.element_mask();
        debug_assert!(value & !mask == 0, "{value} does not fit in {} bits", self.shape.element_bits);
        let (plane, word, shift) = self.locate(tile);
        let slot = &mut self.levels[plane][word];
        *slot = (*slot & !(mask << shift)) | (value << shift);
    }

    /// The tile one level coarser that holds `tile`.
    fn parent_of(&self, tile: Tile) -> Tile {
        let across = self.shape.children_across();
        Tile { level: tile.level - 1, x: tile.x / across, y: tile.y / across }
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
}
