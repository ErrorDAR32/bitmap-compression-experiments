//! The patterns pyramid: every tile from the whole bitmap down to 4x4
//! holds its pattern number -- two tiles of the same size hold the same
//! number exactly when they hold the same cells. Whether two tiles
//! match, what a copy asks, is then one comparison of two numbers, at
//! any size, whatever their cells.
//!
//! Numbers are handed out in the order patterns first appear, a level at
//! a time: 0 is every cell clear, 1 every cell set, then 2, 3, ... for
//! each new pattern, in Morton order. A number says nothing of the cells
//! it stands for, so each level has two tables beside the pyramid:
//!
//! - a reverse lookup, from a pattern to its number: how a tile gets its
//!   number. Keyed by the 4x4's 16 cells, or, coarser, by the four
//!   children's numbers -- a tile is its four children, so equal
//!   children's numbers are equal cells. Those four numbers are four
//!   consecutive 16-bit elements one level finer: one word;
//! - the tile each number first appeared at.
//!
//! Built in one sweep, finest level first: the 4x4s' numbers from the
//! bitmap's words, each coarser level's from the level below. The
//! reverse lookup is a fixed table of slots per level, twice as many as
//! the level has tiles, probed in order from where the pattern's hash
//! lands. Nothing clears it between bitmaps: a slot counts only when it
//! was filled by the current build.

use super::copyable::FINEST_COPY_LEVEL;
use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{tiles_across, tiles_down_to, Tile, CELL_LEVEL};
use crate::morton::morton_coordinates;
use crate::Bitmap;

/// Every cell of the tile clear.
pub const ALL_CLEAR: u16 = 0;
/// Every cell of the tile set.
pub const ALL_SET: u16 = 1;
/// The first number a pattern that is not homogeneous gets.
const FIRST_PATTERN: u16 = 2;

/// The finest level held: 4x4, the finest tile a copy reads.
const FINEST: u8 = FINEST_COPY_LEVEL;
/// Bits a number takes: enough for every 4x4 its own pattern, and the
/// two homogeneous ones.
const NUMBER_BITS: usize = 16;
const _: () = assert!(tiles_across(FINEST) * tiles_across(FINEST) + FIRST_PATTERN as usize <= 1 << NUMBER_BITS);
/// One number, at the bottom of a word.
const NUMBER_MASK: u64 = (1 << NUMBER_BITS) - 1;
/// Numbers a word: a tile's four children's, exactly.
const NUMBERS_A_WORD: usize = u64::BITS as usize / NUMBER_BITS;
const _: () = assert!(NUMBERS_A_WORD == 4);

/// Cells a 4x4 holds: its key, one 16-bit run of the bitmap.
const FINEST_CELLS: usize = 1 << (2 * (CELL_LEVEL - FINEST));
const _: () = assert!(FINEST_CELLS == NUMBER_BITS);

/// A tile's four children all clear, as one word of numbers.
const CHILDREN_ALL_CLEAR: u64 = 0;
/// A tile's four children all set, as one word of numbers.
const CHILDREN_ALL_SET: u64 = 0x0001_0001_0001_0001;

/// Every tile's number, 16 bits a tile, whole bitmap to 4x4.
const SHAPE: PyramidShape = PyramidShape { coarsest_level: 0, finest_level: FINEST, element_bits: NUMBER_BITS };

/// Levels held.
const LEVELS: usize = FINEST as usize + 1;
/// Slots a level's reverse lookup has for each of its tiles: half full at
/// most, so a probe ends soon.
const SLOTS_A_TILE: usize = 2;
/// Every level's numbers together: each level's tiles, and the two
/// homogeneous patterns.
const NUMBERS: usize = tiles_down_to(FINEST) + LEVELS * FIRST_PATTERN as usize;

/// Spreads a key's bits before its top bits pick a slot: the golden
/// ratio's fraction of 2^64, odd, so no two keys collide on it alone.
const HASH_MULTIPLIER: u64 = 0x9E37_79B9_7F4A_7C15;

/// One slot of a reverse lookup.
#[derive(Clone, Copy, Default)]
struct Slot {
    /// The pattern.
    key: u64,
    /// Its number.
    number: u16,
    /// The build that filled it: a slot of any other build is empty.
    build: u32,
}

/// The patterns pyramid and its tables, allocated once.
pub struct Patterns {
    /// Every tile's number.
    numbers: Pyramid,
    /// Every level's reverse lookup, one after another.
    slots: Box<[Slot]>,
    /// Where each level's slots start, and how many it has.
    slot_ranges: [(usize, usize); LEVELS],
    /// For every level and number, the Morton index of the tile it first
    /// appeared at.
    first: Box<[u16]>,
    /// Where each level's first tiles start.
    first_starts: [usize; LEVELS],
    /// How many numbers each level has handed out.
    handed_out: [u16; LEVELS],
    /// The current build, never 0: slots start filled by build 0.
    build: u32,
}

impl Default for Patterns {
    /// Every table allocated, nothing built.
    fn default() -> Self {
        let (mut slot_ranges, mut first_starts) = ([(0, 0); LEVELS], [0; LEVELS]);
        let (mut slot_start, mut first_start) = (0, 0);
        for level in 0..=FINEST {
            let tiles = tiles_across(level) * tiles_across(level);
            let count = (SLOTS_A_TILE * tiles).next_power_of_two();
            slot_ranges[level as usize] = (slot_start, count);
            slot_start += count;
            first_starts[level as usize] = first_start;
            first_start += tiles + FIRST_PATTERN as usize;
        }
        debug_assert_eq!(first_start, NUMBERS);
        Self {
            numbers: Pyramid::new(SHAPE),
            slots: std::iter::repeat_n(Slot::default(), slot_start).collect(),
            slot_ranges,
            first: std::iter::repeat_n(0, first_start).collect(),
            first_starts,
            handed_out: [FIRST_PATTERN; LEVELS],
            build: 0,
        }
    }
}

impl Patterns {
    /// Numbers every tile of `bitmap`, whatever was built before: the
    /// sweep, finest level first.
    pub fn build(&mut self, bitmap: &Bitmap) {
        self.build = self.build.wrapping_add(1);
        if self.build == 0 {
            // Every build number has been used: empty every slot once.
            self.slots.fill(Slot::default());
            self.build = 1;
        }
        self.handed_out = [FIRST_PATTERN; LEVELS];

        // The 4x4s: each one's key its 16 cells, four to a word of the
        // bitmap -- four numbers to a word of the level.
        let cells = bitmap.words();
        for (at, &word) in cells.iter().enumerate() {
            let mut numbers = 0;
            for quarter in 0..NUMBERS_A_WORD {
                let key = word >> (quarter * NUMBER_BITS) & NUMBER_MASK;
                let index = at * NUMBERS_A_WORD + quarter;
                numbers |= (self.intern(FINEST, key, index) as u64) << (quarter * NUMBER_BITS);
            }
            self.numbers.level_words_mut(FINEST)[at] = numbers;
        }

        // Every coarser level: each tile's key its children's word.
        for level in (0..FINEST).rev() {
            for word_at in 0..(tiles_across(level) * tiles_across(level)).div_ceil(NUMBERS_A_WORD) {
                let mut numbers = 0;
                for quarter in 0..NUMBERS_A_WORD {
                    let index = word_at * NUMBERS_A_WORD + quarter;
                    if index >= tiles_across(level) * tiles_across(level) {
                        break;
                    }
                    let key = self.numbers.level_words(level + 1)[index];
                    numbers |= (self.intern(level, key, index) as u64) << (quarter * NUMBER_BITS);
                }
                self.numbers.level_words_mut(level)[word_at] = numbers;
            }
        }
    }

    /// The number of the pattern `key` at `level`, first seen at the tile
    /// whose Morton index is `index`: a homogeneous one's own, one already
    /// handed out, or the next.
    #[inline]
    fn intern(&mut self, level: u8, key: u64, index: usize) -> u16 {
        let (all_clear, all_set) = if level == FINEST { (0, NUMBER_MASK) } else { (CHILDREN_ALL_CLEAR, CHILDREN_ALL_SET) };
        if key == all_clear {
            return ALL_CLEAR;
        }
        if key == all_set {
            return ALL_SET;
        }
        let (start, count) = self.slot_ranges[level as usize];
        let mut at = (key.wrapping_mul(HASH_MULTIPLIER) >> (u64::BITS - count.trailing_zeros())) as usize;
        loop {
            let slot = &mut self.slots[start + at];
            if slot.build != self.build {
                let number = self.handed_out[level as usize];
                self.handed_out[level as usize] += 1;
                *slot = Slot { key, number, build: self.build };
                self.first[self.first_starts[level as usize] + number as usize] = index as u16;
                return number;
            }
            if slot.key == key {
                return slot.number;
            }
            at = (at + 1) & (count - 1);
        }
    }

    /// `tile`'s pattern number: equal for two tiles of the same size
    /// exactly when they hold the same cells.
    #[inline]
    pub fn number(&self, tile: Tile) -> u16 {
        self.numbers.get(tile) as u16
    }

    /// `tile`'s four children's pattern numbers, in reading order.
    #[inline]
    pub fn children_numbers(&self, tile: Tile) -> [u16; 4] {
        let [a, b, c, d] = self.numbers.children_elements(tile);
        [a as u16, b as u16, c as u16, d as u16]
    }

    /// The tile the pattern numbered `number` at `level` first appeared
    /// at, in Morton order; `None` for a number not handed out, or a
    /// homogeneous pattern's.
    pub fn first_tile(&self, level: u8, number: u16) -> Option<Tile> {
        (FIRST_PATTERN..self.handed_out[level as usize]).contains(&number).then(|| {
            let (x, y) = morton_coordinates(self.first[self.first_starts[level as usize] + number as usize] as usize);
            Tile { level, x, y }
        })
    }

    /// How many patterns at `level` are not homogeneous.
    pub fn distinct(&self, level: u8) -> usize {
        (self.handed_out[level as usize] - FIRST_PATTERN) as usize
    }
}
