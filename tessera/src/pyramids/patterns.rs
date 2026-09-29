//! The patterns pyramid: every tile from the whole bitmap down to 4x4
//! holds its pattern number -- two tiles of the same size hold the same
//! number exactly when they hold the same cells. Whether two tiles
//! match, what a copy asks, is then one comparison of two numbers, at
//! any size, whatever their cells -- and whether a tile is homogeneous,
//! all clear or all set, is whether its number is one of theirs.
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
//! lands, emptied at every build. A slot holds only a number, two bytes:
//! the pattern it stands for is read back off the tile that number first
//! appeared at.

use super::copyable::FINEST_COPY_LEVEL;
use super::pyramid::{Pyramid, PyramidShape};
use crate::tile::{tiles_in_level, tiles_down_to, Tile, CELL_LEVEL};
use crate::morton::morton_coordinates;
use crate::Bitmap;

/// What a tile whose pattern number is `number` holds, if every cell of
/// it agrees.
#[inline]
pub fn value_of(number: u16) -> Option<bool> {
    match number {
        ALL_CLEAR => Some(false),
        ALL_SET => Some(true),
        _ => None,
    }
}

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
const _: () = assert!(tiles_in_level(FINEST) + FIRST_PATTERN as usize <= 1 << NUMBER_BITS);
/// One number, at the bottom of a word.
const NUMBER_MASK: u64 = (1 << NUMBER_BITS) - 1;
/// Numbers a word: a tile's four children's, exactly.
const NUMBERS_A_WORD: usize = u64::BITS as usize / NUMBER_BITS;
const _: () = assert!(NUMBERS_A_WORD == 4);

/// Cells a 4x4 holds: its key, one 16-bit run of the bitmap.
const FINEST_CELLS: usize = 1 << (2 * (CELL_LEVEL - FINEST));
const _: () = assert!(FINEST_CELLS == NUMBER_BITS);

/// A 4x4's key when every cell is clear.
const CELLS_ALL_CLEAR: u64 = 0;
/// A 4x4's key when every cell is set.
const CELLS_ALL_SET: u64 = (1 << FINEST_CELLS) - 1;
/// A one at the bottom of every number in a word.
const ONE_IN_EVERY_NUMBER: u64 = u64::MAX / NUMBER_MASK;
/// A tile's four children all clear, as one word of numbers.
const CHILDREN_ALL_CLEAR: u64 = ALL_CLEAR as u64 * ONE_IN_EVERY_NUMBER;
/// A tile's four children all set, as one word of numbers.
const CHILDREN_ALL_SET: u64 = ALL_SET as u64 * ONE_IN_EVERY_NUMBER;

/// Every tile's number, 16 bits a tile, whole bitmap to 4x4.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NumbersShape;

impl PyramidShape for NumbersShape {
    const COARSEST_LEVEL: u8 = 0;
    const FINEST_LEVEL: u8 = FINEST;
    const ELEMENT_BITS: usize = NUMBER_BITS;
}

/// Every tile's pattern number.
type Numbers = Pyramid<NumbersShape, { NumbersShape::WORDS }>;

/// Levels held.
const LEVELS: usize = FINEST as usize + 1;
/// Slots a level's reverse lookup has for each of its tiles: half full at
/// most, so a probe ends soon.
const SLOTS_A_TILE: usize = 2;
/// Where each level's slots start, by level, and where the finest one's
/// end: a power of two for each level, at least twice its tiles.
const SLOT_STARTS: [usize; LEVELS + 1] = {
    let mut starts = [0; LEVELS + 1];
    let mut level = 0;
    while level < LEVELS {
        starts[level + 1] = starts[level] + (SLOTS_A_TILE * tiles_in_level(level as u8)).next_power_of_two();
        level += 1;
    }
    starts
};
/// Slots in every level's reverse lookup together.
const SLOTS: usize = SLOT_STARTS[LEVELS];

/// Where each level's first tiles start, by level, and where the finest
/// one's end: a number for each of its tiles, and the two homogeneous
/// patterns.
const FIRST_STARTS: [usize; LEVELS + 1] = {
    let mut starts = [0; LEVELS + 1];
    let mut level = 0;
    while level < LEVELS {
        starts[level + 1] = starts[level] + tiles_in_level(level as u8) + FIRST_PATTERN as usize;
        level += 1;
    }
    starts
};
/// Every level's numbers together.
const NUMBERS: usize = FIRST_STARTS[LEVELS];
const _: () = assert!(NUMBERS == tiles_down_to(FINEST) + LEVELS * FIRST_PATTERN as usize);

/// Spreads a key's bits before its top bits pick a slot: the golden
/// ratio's fraction of 2^64, odd, so no two keys collide on it alone.
const HASH_MULTIPLIER: u64 = 0x9E37_79B9_7F4A_7C15;

/// A reverse lookup's empty slot: no pattern that is not homogeneous
/// has number 0.
const EMPTY_SLOT: u16 = ALL_CLEAR;
const _: () = assert!(EMPTY_SLOT < FIRST_PATTERN);

/// The patterns pyramid and its tables, allocated once.
pub struct Patterns {
    /// Every tile's number.
    numbers: Numbers,
    /// Every level's reverse lookup, one after another: each slot a
    /// pattern's number, its key read back off the tile it first
    /// appeared at.
    slots: Box<[u16; SLOTS]>,
    /// For every level and number, the Morton index of the tile it first
    /// appeared at.
    first: Box<[u16; NUMBERS]>,
    /// How many numbers each level has handed out.
    handed_out: [u16; LEVELS],
    /// For every level and number, whether a second tile has it: a bit
    /// a number, as `first` counts them.
    repeated: [u64; NUMBERS.div_ceil(u64::BITS as usize)],
}

impl Default for Patterns {
    /// Every table allocated, nothing built.
    fn default() -> Self {
        Self {
            numbers: Numbers::new(),
            slots: Box::new([EMPTY_SLOT; SLOTS]),
            first: Box::new([0; NUMBERS]),
            handed_out: [FIRST_PATTERN; LEVELS],
            repeated: [0; NUMBERS.div_ceil(u64::BITS as usize)],
        }
    }
}

impl Patterns {
    /// Numbers every tile of `bitmap`, whatever was built before: the
    /// sweep, finest level first.
    pub fn build(&mut self, bitmap: &Bitmap) {
        self.slots.fill(EMPTY_SLOT);
        self.handed_out = [FIRST_PATTERN; LEVELS];
        self.repeated.fill(0);

        for level in (0..=FINEST).rev() {
            let tiles = tiles_in_level(level);
            for word_index in 0..tiles.div_ceil(NUMBERS_A_WORD) {
                let mut numbers = 0;
                for quarter in 0..NUMBERS_A_WORD.min(tiles) {
                    let tile_index = word_index * NUMBERS_A_WORD + quarter;
                    let key = self.key(bitmap, level, tile_index);
                    numbers |= (self.intern(bitmap, level, key, tile_index) as u64) << (quarter * NUMBER_BITS);
                }
                self.numbers.level_words_mut(level)[word_index] = numbers;
            }
        }
    }

    /// The key of the tile at `level` whose Morton index is `tile_index`:
    /// a 4x4's 16 cells, one run of the bitmap, or a coarser tile's four
    /// children's numbers, one word of the level below.
    #[inline]
    fn key(&self, bitmap: &Bitmap, level: u8, tile_index: usize) -> u64 {
        if level == FINEST {
            bitmap.morton_run(tile_index * FINEST_CELLS, FINEST_CELLS)
        } else {
            self.numbers.level_words(level + 1)[tile_index]
        }
    }

    /// The number of the pattern `key` at `level`, first seen at the tile
    /// whose Morton index is `tile_index`: a homogeneous one's own, one
    /// already handed out, or the next.
    #[inline]
    fn intern(&mut self, bitmap: &Bitmap, level: u8, key: u64, tile_index: usize) -> u16 {
        let (all_clear, all_set) = if level == FINEST { (CELLS_ALL_CLEAR, CELLS_ALL_SET) } else { (CHILDREN_ALL_CLEAR, CHILDREN_ALL_SET) };
        if key == all_clear {
            return ALL_CLEAR;
        }
        if key == all_set {
            return ALL_SET;
        }
        let (first_slot, slot_count) = (SLOT_STARTS[level as usize], SLOT_STARTS[level as usize + 1] - SLOT_STARTS[level as usize]);
        let mut probe = (key.wrapping_mul(HASH_MULTIPLIER) >> (u64::BITS - slot_count.trailing_zeros())) as usize;
        let first_start = FIRST_STARTS[level as usize];
        loop {
            let number = self.slots[first_slot + probe];
            if number == EMPTY_SLOT {
                let number = self.handed_out[level as usize];
                self.handed_out[level as usize] += 1;
                self.slots[first_slot + probe] = number;
                self.first[first_start + number as usize] = tile_index as u16;
                return number;
            }
            if self.key(bitmap, level, self.first[first_start + number as usize] as usize) == key {
                let index = first_start + number as usize;
                self.repeated[index / u64::BITS as usize] |= 1 << (index % u64::BITS as usize);
                return number;
            }
            probe = (probe + 1) & (slot_count - 1);
        }
    }

    /// `tile`'s pattern number: equal for two tiles of the same size
    /// exactly when they hold the same cells.
    #[inline]
    pub fn number(&self, tile: Tile) -> u16 {
        self.numbers.get(tile) as u16
    }

    /// Whether another tile of `level` holds the pattern numbered
    /// `number` -- always, for a homogeneous one: only then can a tile
    /// with it be a copy.
    #[inline]
    pub fn repeats(&self, level: u8, number: u16) -> bool {
        let index = FIRST_STARTS[level as usize] + number as usize;
        number < FIRST_PATTERN || self.repeated[index / u64::BITS as usize] >> (index % u64::BITS as usize) & 1 == 1
    }

    /// What `tile` holds, if every cell of it agrees: the homogeneous
    /// patterns have numbers of their own.
    #[inline]
    pub fn homogeneous_value(&self, tile: Tile) -> Option<bool> {
        value_of(self.number(tile))
    }

    /// What each of `tile`'s four children holds, in reading order, if
    /// every cell of it agrees.
    #[inline]
    pub fn children_values(&self, tile: Tile) -> [Option<bool>; 4] {
        self.children_numbers(tile).map(value_of)
    }

    /// `tile`'s four children's pattern numbers, in reading order.
    #[inline]
    pub fn children_numbers(&self, tile: Tile) -> [u16; 4] {
        self.numbers.children_elements(tile).map(|number| number as u16)
    }

    /// The tile the pattern numbered `number` at `level` first appeared
    /// at, in Morton order; `None` for a number not handed out, or a
    /// homogeneous pattern's.
    pub fn first_tile(&self, level: u8, number: u16) -> Option<Tile> {
        (FIRST_PATTERN..self.handed_out[level as usize]).contains(&number).then(|| {
            let (x, y) = morton_coordinates(self.first[FIRST_STARTS[level as usize] + number as usize] as usize);
            Tile { level, x, y }
        })
    }

    /// How many patterns at `level` are not homogeneous.
    pub fn distinct(&self, level: u8) -> usize {
        (self.handed_out[level as usize] - FIRST_PATTERN) as usize
    }
}
