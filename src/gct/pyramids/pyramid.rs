//! A generic pyramid: one element per tile, at every level between a
//! coarsest and a finest, each element a fixed number of bits, packed
//! into machine words -- several to a word, or, for an element wider
//! than a word, several words to it.
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
//! three parameters and supplies its own queries -- and, if its coarser
//! levels follow from its finer ones, its own sweep, written for its own
//! elements: its elements are set, then the sweep brings every coarser
//! level in step at once, each tile once. The generic pyramid has no
//! sweep of its own: setting an element never changes any other.

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
    /// words -- or, for an element wider than a word, is a whole number
    /// of words.
    pub element_bits: usize,
}

/// One level's elements, packed into words in Morton order: element `i`
/// is bits `i * element_bits..` of the whole run.
pub type LevelWords = [u64];

/// Every level's start, and one past the finest's end.
const LEVEL_STARTS: usize = CELL_LEVEL as usize + 2;

/// One element per tile, per level -- see the module doc.
#[derive(Clone, Debug)]
pub struct Pyramid {
    /// The three parameters it was built from.
    shape: PyramidShape,
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
    /// Words an element takes: 1 for one a word or narrower.
    words_per_element: usize,
}

/// Two pyramids are equal when they hold the same elements in the same
/// shape.
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
        let word_bits = u64::BITS as usize;
        assert!(
            shape.element_bits > 0 && (word_bits.is_multiple_of(shape.element_bits) || shape.element_bits.is_multiple_of(word_bits)),
            "an element's bits divide a word, or are whole words"
        );
        assert!(shape.coarsest_level <= shape.finest_level);
        let per_word = (word_bits / shape.element_bits).max(1);
        let words_per_element = shape.element_bits.div_ceil(word_bits);
        let mut level_starts = [0; LEVEL_STARTS];
        for level in shape.coarsest_level..=shape.finest_level {
            let elements = tiles_across(level).pow(2);
            level_starts[level as usize + 1] = level_starts[level as usize] + elements.div_ceil(per_word) * words_per_element;
        }
        let element_mask = if shape.element_bits >= word_bits { u64::MAX } else { (1 << shape.element_bits) - 1 };
        Self {
            shape,
            words: std::iter::repeat_n(0, level_starts[shape.finest_level as usize + 1]).collect(),
            level_starts,
            per_word_shift: per_word.trailing_zeros(),
            element_mask,
            words_per_element,
        }
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
    /// For elements of a word or narrower.
    #[inline]
    fn locate(&self, tile: Tile) -> (usize, usize) {
        debug_assert!(self.holds(tile), "{tile:?} is outside this pyramid's levels");
        debug_assert!(self.words_per_element == 1, "an element wider than a word is read by its words");
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

    /// The words a tile's four children fill between them, for elements
    /// of 16 bits or more: the children's elements in reading order,
    /// packed as always.
    #[inline]
    pub fn children_words(&self, tile: Tile) -> &[u64] {
        debug_assert!(self.shape.element_bits * 4 >= u64::BITS as usize, "four children fill whole words");
        let words = self.shape.element_bits * 4 / u64::BITS as usize;
        let first = self.level_starts[tile.level as usize + 1] + morton_index(tile.x, tile.y) * words;
        &self.words[first..first + words]
    }

    /// Replaces a tile's element, nothing else.
    #[inline]
    pub fn set(&mut self, tile: Tile, value: u64) {
        let mask = self.element_mask;
        debug_assert!(value & !mask == 0, "{value} does not fit in {} bits", self.shape.element_bits);
        let (word, shift) = self.locate(tile);
        let slot = &mut self.words[word];
        *slot = (*slot & !(mask << shift)) | (value << shift);
    }

    /// A tile's element of one word or more: its words. In Morton order,
    /// a tile's four children's are next to each other.
    #[inline]
    pub fn element_words(&self, tile: Tile) -> &[u64] {
        let first = self.first_word_of(tile);
        &self.words[first..first + self.words_per_element]
    }

    /// A tile's element of one word or more, to write.
    #[inline]
    pub fn element_words_mut(&mut self, tile: Tile) -> &mut [u64] {
        let first = self.first_word_of(tile);
        &mut self.words[first..first + self.words_per_element]
    }

    /// The first word of a tile's element of one word or more.
    #[inline]
    fn first_word_of(&self, tile: Tile) -> usize {
        debug_assert!(self.holds(tile), "{tile:?} is outside this pyramid's levels");
        debug_assert!(self.shape.element_bits.is_multiple_of(u64::BITS as usize), "an element narrower than a word shares it");
        self.level_starts[tile.level as usize] + morton_index(tile.x, tile.y) * self.words_per_element
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

    /// Every tile of one level, in Morton order, as the level is laid out.
    pub fn tiles_of_level(&self, level: u8) -> impl Iterator<Item = Tile> {
        Tile::all_of_level(level)
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

    /// A level's words, to write directly. Past the
    /// level's last element, a word's bits must stay zero.
    pub fn level_words_mut(&mut self, level: u8) -> &mut LevelWords {
        &mut self.words[self.level_starts[level as usize]..self.level_starts[level as usize + 1]]
    }

}
