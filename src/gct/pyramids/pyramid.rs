//! A generic pyramid: one element per tile, at every level between a
//! coarsest and a finest, each element a fixed number of bits, packed
//! into machine words.
//!
//! A tile is its level and its (x, y) in that level's plane; a tile's
//! children are the 2x2 block at (2x..2x+1, 2y..2y+1) one level finer.
//! Each level's plane is stored in Morton order (`src/morton.rs`), so a
//! tile's four children are four consecutive elements, and everything
//! under a tile at any level is one contiguous run. A specialized
//! pyramid may read and write a level's words directly
//! ([`Pyramid::level_words`]), to build a whole level a word at a time.
//!
//! A specialized pyramid (the other files in this folder) fixes the
//! three parameters, supplies its own queries, and may give the pyramid
//! a [`Propagation`]: the rule for what a tile holds, given its children.
//! Then every [`Pyramid::set`] keeps the coarser levels in step on its
//! own: it recomputes the set tile's parent, then that one's parent, and
//! stops at the first whose element does not change -- often right
//! away, sometimes only at the whole bitmap.

use crate::gct::tile::{tiles_across, Tile, CELL_LEVEL, CHILDREN_ACROSS};
use crate::morton::morton_index;

/// The three parameters every pyramid is built from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PyramidShape {
    /// The coarsest level held.
    pub coarsest_level: u8,
    /// The finest level held.
    pub finest_level: u8,
    /// Bits per element. Divides 64, so an element never straddles two
    /// words.
    pub element_bits: usize,
}

/// One level's elements, packed into words in Morton order: element `i`
/// is bits `i * element_bits..` of the whole run.
pub type LevelWords = [u64];

/// Every level's start, and one past the finest's end.
const LEVEL_STARTS: usize = CELL_LEVEL as usize + 2;

/// What `tile` should hold, worked out from its children's elements in
/// `pyramid`.
pub type Propagation = fn(pyramid: &Pyramid, tile: Tile) -> u64;

/// One element per tile, per level -- see the module doc.
#[derive(Clone, Debug)]
pub struct Pyramid {
    /// The three parameters it was built from.
    shape: PyramidShape,
    /// What keeps the coarser levels in step on every set, if anything.
    propagation: Option<Propagation>,
    /// Every level's words, coarsest first, each level starting a word
    /// of its own: as many as the shape needs, fixed when it is built.
    words: Box<[u64]>,
    /// Where each level's words start in `words`, by level, and where
    /// the last one's end; held inline, one hop less on every access.
    level_starts: [usize; LEVEL_STARTS],
    /// Elements a word is a power of two -- an element's bits divide a
    /// word's -- so a tile's word and place in it are shifts and masks.
    per_word_shift: u32,
    /// One element's bits, at the bottom of a word.
    element_mask: u64,
}

/// Two pyramids are equal when they hold the same elements in the same
/// shape; how they propagate is behaviour, not content.
impl PartialEq for Pyramid {
    fn eq(&self, other: &Self) -> bool {
        self.shape == other.shape && self.words == other.words
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
        let mut level_starts = [0; LEVEL_STARTS];
        for level in shape.coarsest_level..=shape.finest_level {
            let elements = tiles_across(level).pow(2);
            level_starts[level as usize + 1] = level_starts[level as usize] + elements.div_ceil(per_word);
        }
        let element_mask =
            if shape.element_bits == u64::BITS as usize { u64::MAX } else { (1 << shape.element_bits) - 1 };
        Self {
            shape,
            propagation: None,
            words: std::iter::repeat_n(0, level_starts[shape.finest_level as usize + 1]).collect(),
            level_starts,
            per_word_shift: per_word.trailing_zeros(),
            element_mask,
        }
    }

    /// An all-zero pyramid of this shape that keeps its coarser levels
    /// in step with `propagation`. All zeros must already be in step:
    /// `propagation` of all-zero children is zero.
    pub fn with_propagation(shape: PyramidShape, propagation: Propagation) -> Self {
        Self { propagation: Some(propagation), ..Self::new(shape) }
    }

    /// Sets every element back to zero, keeping the room they take.
    pub fn clear(&mut self) {
        self.words.fill(0);
    }

    /// The three parameters it was built from.
    pub fn shape(&self) -> PyramidShape {
        self.shape
    }

    /// Whether this pyramid holds `tile`'s level at all.
    pub fn holds(&self, tile: Tile) -> bool {
        (self.shape.coarsest_level..=self.shape.finest_level).contains(&tile.level)
    }

    /// Where a tile's element sits: its word, and the shift within it.
    #[inline]
    fn locate(&self, tile: Tile) -> (usize, usize) {
        debug_assert!(self.holds(tile), "{tile:?} is outside this pyramid's levels");
        let index = morton_index(tile.x, tile.y);
        let in_word = index & ((1 << self.per_word_shift) - 1);
        (self.level_starts[tile.level as usize] + (index >> self.per_word_shift), in_word * self.shape.element_bits)
    }

    /// A tile's element.
    #[inline]
    pub fn get(&self, tile: Tile) -> u64 {
        let (word, shift) = self.locate(tile);
        (self.words[word] >> shift) & self.element_mask
    }

    /// A tile's four children's elements, in reading order -- which, for
    /// one 2x2 group, is Morton order: four consecutive elements, found
    /// with one lookup.
    #[inline]
    pub fn children_elements(&self, tile: Tile) -> [u64; 4] {
        debug_assert!(self.holds(Tile { level: tile.level + 1, ..tile }), "{tile:?}'s children are outside this pyramid's levels");
        let first = morton_index(tile.x, tile.y) * 4;
        let start = self.level_starts[tile.level as usize + 1];
        let mut elements = [0; 4];
        for (child, element) in elements.iter_mut().enumerate() {
            let index = first + child;
            let shift = (index & ((1 << self.per_word_shift) - 1)) * self.shape.element_bits;
            *element = (self.words[start + (index >> self.per_word_shift)] >> shift) & self.element_mask;
        }
        elements
    }

    /// Replaces a tile's element, then propagates: each coarser tile
    /// holding it is recomputed, up to the first that does not change.
    #[inline]
    pub fn set(&mut self, tile: Tile, value: u64) {
        self.write(tile, value);
        if let Some(propagation) = self.propagation {
            self.propagate_from(tile, propagation);
        }
    }

    /// Recomputes each coarser tile holding `tile` by `propagation`, up
    /// to the first that does not change. Kept out of line: most
    /// pyramids do not propagate, and their sets stay a few steps.
    #[inline(never)]
    fn propagate_from(&mut self, tile: Tile, propagation: Propagation) {
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
    #[inline]
    fn write(&mut self, tile: Tile, value: u64) {
        let mask = self.element_mask;
        debug_assert!(value & !mask == 0, "{value} does not fit in {} bits", self.shape.element_bits);
        let (word, shift) = self.locate(tile);
        let slot = &mut self.words[word];
        *slot = (*slot & !(mask << shift)) | (value << shift);
    }

    /// The tile one level coarser that holds `tile`.
    fn parent_of(&self, tile: Tile) -> Tile {
        let across = CHILDREN_ACROSS;
        Tile { level: tile.level - 1, x: tile.x / across, y: tile.y / across }
    }

    /// A tile's children, in reading order.
    pub fn children_of(&self, tile: Tile) -> impl Iterator<Item = Tile> {
        let across = CHILDREN_ACROSS as usize;
        (0..across).flat_map(move |row| {
            (0..across).map(move |col| {
                let (x, y) = (tile.x as usize * across + col, tile.y as usize * across + row);
                Tile { level: tile.level + 1, x: x as u8, y: y as u8 }
            })
        })
    }

    /// Every tile of one level, in reading order.
    pub fn tiles_of_level(&self, level: u8) -> impl Iterator<Item = Tile> {
        let last = (tiles_across(level) - 1) as u8;
        (0..=last).flat_map(move |y| (0..=last).map(move |x| Tile { level, x, y }))
    }

    /// A level's elements. Past the level's last element, a word's
    /// bits are zero.
    pub fn level_words(&self, level: u8) -> &LevelWords {
        &self.words[self.level_starts[level as usize]..self.level_starts[level as usize + 1]]
    }

    /// `level`'s words to write, and the next finer level's to read --
    /// for building a level from the one below it.
    pub fn two_levels_mut(&mut self, level: u8) -> (&mut LevelWords, &LevelWords) {
        let (start, split, end) = (
            self.level_starts[level as usize],
            self.level_starts[level as usize + 1],
            self.level_starts[level as usize + 2],
        );
        let (coarser, finer) = self.words[start..end].split_at_mut(split - start);
        (coarser, finer)
    }

    /// `level`'s words to read, and the next finer level's to write --
    /// for handing something down from a level to the one below it.
    pub fn finer_level_mut(&mut self, level: u8) -> (&LevelWords, &mut LevelWords) {
        let (start, split, end) = (
            self.level_starts[level as usize],
            self.level_starts[level as usize + 1],
            self.level_starts[level as usize + 2],
        );
        let (coarser, finer) = self.words[start..end].split_at_mut(split - start);
        (coarser, finer)
    }

    /// A level's words, to write directly; nothing propagates. Past the
    /// level's last element, a word's bits must stay zero.
    pub fn level_words_mut(&mut self, level: u8) -> &mut LevelWords {
        &mut self.words[self.level_starts[level as usize]..self.level_starts[level as usize + 1]]
    }

}
