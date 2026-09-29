//! A generic pyramid: one element per tile, at every level between a
//! coarsest and a finest, each element a fixed power-of-two number of
//! bits, packed into machine words with no gaps between them -- several
//! to a word, or, for an element wider than a word, several words to it.
//!
//! A tile is its level and its (x, y) in that level's plane; a tile's
//! children are the 2x2 block at (2x..2x+1, 2y..2y+1) one level finer.
//! Each level's plane is stored in Morton order ([`bitmap::morton`]), so a
//! tile's four children are four consecutive elements, and everything
//! under a tile at any level is one contiguous run. A specialized
//! pyramid may read and write a level's words directly
//! ([`Pyramid::level_words`]), to build a whole level a word at a time.
//!
//! A specialized pyramid (the other files in this folder) is a
//! [`PyramidShape`] -- its levels and its element's bits, all constants
//! -- and a type alias naming the pyramid of that shape, with its own
//! queries -- and, if its coarser levels follow from its finer ones, its
//! own sweep, written for its own elements: its elements are set, then
//! the sweep brings every coarser level in step at once, each tile once.
//! The generic pyramid has no sweep of its own: setting an element never
//! changes any other.
//!
//! Every size is a constant: the words the pyramid takes, where each
//! level starts, how elements sit in a word. So its storage is one array
//! whose length is known when compiling, allocated once.

use std::marker::PhantomData;

use crate::tile::{tiles_in_level, Tile, CELL_LEVEL};
use bitmap::morton::morton_index;

/// Bits in a word.
const WORD_BITS: usize = u64::BITS as usize;

/// What a specialized pyramid is: the levels it holds and its element's
/// bits, every one a constant.
pub trait PyramidShape {
    /// The coarsest level held.
    const COARSEST_LEVEL: u8;
    /// The finest level held.
    const FINEST_LEVEL: u8;
    /// Bits per element: a power of two, so an element never straddles
    /// two words -- or, for an element wider than a word, is a whole
    /// number of words.
    const ELEMENT_BITS: usize;
    /// The words a pyramid of this shape takes: every level's, each
    /// level starting a word of its own.
    const WORDS: usize = level_starts(Self::COARSEST_LEVEL, Self::FINEST_LEVEL, Self::ELEMENT_BITS)[Self::FINEST_LEVEL as usize + 1];
}

/// Every level's start, and one past the finest's end.
const LEVEL_STARTS: usize = CELL_LEVEL as usize + 2;

/// Where each level's words start in a pyramid of these levels and
/// element bits, by level, and where the finest one's end: levels
/// coarser than `coarsest` take no words.
pub const fn level_starts(coarsest: u8, finest: u8, element_bits: usize) -> [usize; LEVEL_STARTS] {
    let mut starts = [0; LEVEL_STARTS];
    let mut level = coarsest;
    while level <= finest {
        starts[level as usize + 1] = starts[level as usize] + (tiles_in_level(level) * element_bits).div_ceil(WORD_BITS);
        level += 1;
    }
    starts
}

/// One level's elements, packed into words in Morton order: element `i`
/// is bits `i * element_bits..` of the whole run.
pub type LevelWords = [u64];

/// One element per tile, per level, of the shape `S` -- see the module
/// doc. `WORDS` is always `S::WORDS`: the length of the one array it is
/// stored in, which a type alias names.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Pyramid<S: PyramidShape, const WORDS: usize> {
    /// Every level's words, coarsest first, each level starting a word
    /// of its own.
    words: Box<[u64; WORDS]>,
    /// The shape: nothing held, every size a constant.
    shape: PhantomData<S>,
}

impl<S: PyramidShape, const WORDS: usize> Pyramid<S, WORDS> {
    /// The shape is sound: its words are `WORDS`, its levels in order,
    /// and its element a power of two bits, so elements pack words with
    /// no gaps.
    const SOUND: () = {
        assert!(WORDS == S::WORDS, "a pyramid's words are its shape's");
        assert!(S::COARSEST_LEVEL <= S::FINEST_LEVEL && S::FINEST_LEVEL <= CELL_LEVEL);
        assert!(S::ELEMENT_BITS.is_power_of_two(), "an element's bits are a power of two");
    };
    /// Where each level's words start, by level.
    const LEVEL_STARTS: [usize; LEVEL_STARTS] = level_starts(S::COARSEST_LEVEL, S::FINEST_LEVEL, S::ELEMENT_BITS);
    /// Elements a word: a power of two, at least one, so a tile's word
    /// and place in it are shifts and masks.
    const ELEMENTS_A_WORD: usize = if S::ELEMENT_BITS >= WORD_BITS { 1 } else { WORD_BITS / S::ELEMENT_BITS };
    /// One element's bits, at the bottom of a word.
    const ELEMENT_MASK: u64 = if S::ELEMENT_BITS >= WORD_BITS { u64::MAX } else { (1 << S::ELEMENT_BITS) - 1 };
    /// Words an element takes: 1 for one a word or narrower.
    const WORDS_PER_ELEMENT: usize = S::ELEMENT_BITS.div_ceil(WORD_BITS);

    /// An all-zero pyramid, whose levels are independent: setting a tile
    /// changes nothing else.
    pub fn new() -> Self {
        let () = Self::SOUND;
        Self { words: Box::new([0; WORDS]), shape: PhantomData }
    }

    /// Sets every element back to zero.
    pub fn clear(&mut self) {
        self.words.fill(0);
    }

    /// Whether this pyramid holds `tile`'s level at all.
    fn holds(tile: Tile) -> bool {
        (S::COARSEST_LEVEL..=S::FINEST_LEVEL).contains(&tile.level)
    }

    /// Where a tile's element sits: its word, and the shift within it.
    /// For elements of a word or narrower.
    #[inline]
    fn locate(tile: Tile) -> (usize, usize) {
        debug_assert!(Self::holds(tile), "{tile:?} is outside this pyramid's levels");
        let index = morton_index(tile.x, tile.y);
        (Self::LEVEL_STARTS[tile.level as usize] + index / Self::ELEMENTS_A_WORD, index % Self::ELEMENTS_A_WORD * S::ELEMENT_BITS)
    }

    /// A tile's element, of a word or narrower.
    #[inline]
    pub fn get(&self, tile: Tile) -> u64 {
        let (word, shift) = Self::locate(tile);
        (self.words[word] >> shift) & Self::ELEMENT_MASK
    }

    /// A tile's four children's elements, of a word or narrower, in
    /// reading order -- which, for one 2x2 group, is Morton order: four
    /// consecutive elements, found with one lookup.
    #[inline]
    pub fn children_elements(&self, tile: Tile) -> [u64; 4] {
        debug_assert!(Self::holds(Tile { level: tile.level + 1, ..tile }), "{tile:?}'s children are outside this pyramid's levels");
        let first_child_index = morton_index(tile.x, tile.y) * 4;
        let finer_level_start = Self::LEVEL_STARTS[tile.level as usize + 1];
        std::array::from_fn(|child| {
            let index = first_child_index + child;
            let shift = index % Self::ELEMENTS_A_WORD * S::ELEMENT_BITS;
            (self.words[finer_level_start + index / Self::ELEMENTS_A_WORD] >> shift) & Self::ELEMENT_MASK
        })
    }

    /// The words a tile's four children fill between them, for elements
    /// of 16 bits or more: the children's elements in reading order,
    /// packed as always.
    #[inline]
    pub fn children_words(&self, tile: Tile) -> &[u64] {
        debug_assert!(S::ELEMENT_BITS * 4 >= WORD_BITS, "four children fill whole words");
        let words = S::ELEMENT_BITS * 4 / WORD_BITS;
        let first_word = Self::LEVEL_STARTS[tile.level as usize + 1] + morton_index(tile.x, tile.y) * words;
        &self.words[first_word..first_word + words]
    }

    /// The words a tile's four children fill between them, to write, for
    /// elements of 16 bits or more.
    #[inline]
    pub fn children_words_mut(&mut self, tile: Tile) -> &mut [u64] {
        debug_assert!(S::ELEMENT_BITS * 4 >= WORD_BITS, "four children fill whole words");
        let words = S::ELEMENT_BITS * 4 / WORD_BITS;
        let first_word = Self::LEVEL_STARTS[tile.level as usize + 1] + morton_index(tile.x, tile.y) * words;
        &mut self.words[first_word..first_word + words]
    }

    /// Replaces a tile's element, of a word or narrower, nothing else.
    #[inline]
    pub fn set(&mut self, tile: Tile, value: u64) {
        debug_assert!(value & !Self::ELEMENT_MASK == 0, "{value} does not fit in {} bits", S::ELEMENT_BITS);
        let (word, shift) = Self::locate(tile);
        let slot = &mut self.words[word];
        *slot = (*slot & !(Self::ELEMENT_MASK << shift)) | (value << shift);
    }

    /// A tile's element of one word or more: its words. In Morton order,
    /// a tile's four children's are next to each other.
    #[inline]
    pub fn element_words(&self, tile: Tile) -> &[u64] {
        let first_word = Self::first_word_of(tile);
        &self.words[first_word..first_word + Self::WORDS_PER_ELEMENT]
    }

    /// A tile's element of one word or more, to write.
    #[inline]
    pub fn element_words_mut(&mut self, tile: Tile) -> &mut [u64] {
        let first_word = Self::first_word_of(tile);
        &mut self.words[first_word..first_word + Self::WORDS_PER_ELEMENT]
    }

    /// The first word of a tile's element of one word or more.
    #[inline]
    fn first_word_of(tile: Tile) -> usize {
        debug_assert!(Self::holds(tile), "{tile:?} is outside this pyramid's levels");
        debug_assert!(S::ELEMENT_BITS >= WORD_BITS, "an element narrower than a word shares it");
        Self::LEVEL_STARTS[tile.level as usize] + morton_index(tile.x, tile.y) * Self::WORDS_PER_ELEMENT
    }

    /// A level's elements. Past the level's last element, a word's
    /// bits are zero.
    pub fn level_words(&self, level: u8) -> &LevelWords {
        &self.words[Self::LEVEL_STARTS[level as usize]..Self::LEVEL_STARTS[level as usize + 1]]
    }

    /// A level's words, to write directly. Past the level's last
    /// element, a word's bits must stay zero.
    pub fn level_words_mut(&mut self, level: u8) -> &mut LevelWords {
        &mut self.words[Self::LEVEL_STARTS[level as usize]..Self::LEVEL_STARTS[level as usize + 1]]
    }

    /// `level`'s words and the next finer level's, both writable: for
    /// building a level from the one below it, or handing something down
    /// from a level to the one below it.
    pub fn level_and_finer_mut(&mut self, level: u8) -> (&mut LevelWords, &mut LevelWords) {
        let [start, finer_start, end] = [0, 1, 2].map(|past| Self::LEVEL_STARTS[level as usize + past]);
        self.words[start..end].split_at_mut(finer_start - start)
    }
}

impl<S: PyramidShape, const WORDS: usize> Default for Pyramid<S, WORDS> {
    /// The same as [`Pyramid::new`]: all zero.
    fn default() -> Self {
        Self::new()
    }
}
