//! The pattern pyramid: a number per tile, whole bitmap to 4x4, equal for
//! two tiles of one size exactly when they hold the same cells -- so
//! whether a tile is homogeneous, and whether it is copyable, is one
//! comparison. `docs/tessera.md`, "The pattern pyramid".
//!
//! Function by function: `docs/reference.md`, "`patterns.rs`".

use crate::tile::{cells_in_tile, copy_offset, tiles_in_level, Pyramid, Tile, FLOOR_LEVEL};
use bitmap::Bitmap;

/// Every cell of the tile clear.
pub const ALL_CLEAR: u16 = 0;
/// Every cell of the tile set.
pub const ALL_SET: u16 = 1;
/// The first number a pattern that is not homogeneous gets.
const FIRST_PATTERN: u16 = 2;

/// What a tile whose pattern number is `number` holds, if every cell of
/// it agrees.
#[inline]
pub fn homogeneous_value_of(number: u16) -> Option<bool> {
    match number {
        ALL_CLEAR => Some(false),
        ALL_SET => Some(true),
        _ => None,
    }
}

/// Levels numbered: the whole bitmap to the 4x4 floor.
const LEVELS: usize = FLOOR_LEVEL as usize + 1;
/// A 4x4's cells: its pattern, one run of the bitmap.
const FLOOR_CELLS: usize = cells_in_tile(FLOOR_LEVEL);
const _: () = assert!(tiles_in_level(FLOOR_LEVEL) + FIRST_PATTERN as usize <= 1 << u16::BITS, "every number fits a u16");
/// Bits a number takes in a coarser tile's pattern.
const NUMBER_BITS: usize = u16::BITS as usize;
/// A coarser tile's pattern when all four children are clear...
const CHILDREN_ALL_CLEAR: u64 = 0;
/// ...and when all four are set.
const CHILDREN_ALL_SET: u64 = 0x0001_0001_0001_0001 * ALL_SET as u64;

/// Where each level's slots start, and where the finest one's end: a
/// power of two for each level, at least twice its tiles, so a probe
/// ends soon.
const SLOT_STARTS: [usize; LEVELS + 1] = {
    let mut starts = [0; LEVELS + 1];
    let mut level = 0;
    while level < LEVELS {
        starts[level + 1] = starts[level] + (2 * tiles_in_level(level as u8)).next_power_of_two();
        level += 1;
    }
    starts
};
/// Where each level's numbers start among every level's, and where the
/// finest one's end: one for each of its tiles, and the two homogeneous
/// ones.
const NUMBER_STARTS: [usize; LEVELS + 1] = {
    let mut starts = [0; LEVELS + 1];
    let mut level = 0;
    while level < LEVELS {
        starts[level + 1] = starts[level] + tiles_in_level(level as u8) + FIRST_PATTERN as usize;
        level += 1;
    }
    starts
};
/// Every level's numbers together.
const NUMBERS: usize = NUMBER_STARTS[LEVELS];

/// Spreads a pattern's bits before its top bits pick a slot: the golden
/// ratio's fraction of 2^64.
const HASH_MULTIPLIER: u64 = 0x9E37_79B9_7F4A_7C15;
/// An empty slot: no pattern that is not homogeneous has number 0.
const EMPTY_SLOT: u16 = ALL_CLEAR;

/// Every tile's pattern number, and the tables that hand them out.
pub struct Patterns {
    /// Every tile's number.
    numbers: Pyramid<u16, FLOOR_LEVEL>,
    /// Every level's hash table, one after another.
    slots: Box<[u16]>,
    /// For every level and number, the Morton index of the tile it first
    /// appeared at.
    first_tile: Box<[u16]>,
    /// How many numbers each level has handed out.
    handed_out: [u16; LEVELS],
    /// For every level and number, whether a second tile has it.
    repeated: [u64; NUMBERS.div_ceil(u64::BITS as usize)],
}

impl Patterns {
    /// Every table allocated, nothing numbered.
    pub fn new() -> Self {
        Self {
            numbers: Pyramid::new(),
            slots: vec![EMPTY_SLOT; SLOT_STARTS[LEVELS]].into_boxed_slice(),
            first_tile: vec![0; NUMBERS].into_boxed_slice(),
            handed_out: [FIRST_PATTERN; LEVELS],
            repeated: [0; NUMBERS.div_ceil(u64::BITS as usize)],
        }
    }

    /// Numbers every tile of `bitmap`, finest level first.
    pub fn build(&mut self, bitmap: &Bitmap) {
        self.slots.fill(EMPTY_SLOT);
        self.handed_out = [FIRST_PATTERN; LEVELS];
        self.repeated.fill(0);
        for level in (0..=FLOOR_LEVEL).rev() {
            for index in 0..tiles_in_level(level) {
                let number = self.number_for(bitmap, level, index);
                self.numbers.set_at(level, index, number);
            }
        }
    }

    /// The pattern of `level`'s tile at Morton index `index`: a 4x4's
    /// cells, or a coarser tile's four children's numbers.
    #[inline]
    fn pattern_key(&self, bitmap: &Bitmap, level: u8, index: usize) -> u64 {
        if level == FLOOR_LEVEL {
            return bitmap.morton_run(index * FLOOR_CELLS, FLOOR_CELLS);
        }
        let children = self.numbers.children_at(level, index);
        (0..4).fold(0, |key, child| key | (children[child] as u64) << (child * NUMBER_BITS))
    }

    /// The number of `level`'s tile at Morton index `index`: a
    /// homogeneous pattern's own, one already handed out, or the next.
    #[inline]
    fn number_for(&mut self, bitmap: &Bitmap, level: u8, index: usize) -> u16 {
        let key = self.pattern_key(bitmap, level, index);
        let (all_clear, all_set) = if level == FLOOR_LEVEL { (0, (1 << FLOOR_CELLS) - 1) } else { (CHILDREN_ALL_CLEAR, CHILDREN_ALL_SET) };
        if key == all_clear {
            return ALL_CLEAR;
        }
        if key == all_set {
            return ALL_SET;
        }
        let level_index = level as usize;
        let (first_slot, slot_count) = (SLOT_STARTS[level_index], SLOT_STARTS[level_index + 1] - SLOT_STARTS[level_index]);
        let first_number = NUMBER_STARTS[level_index];
        let mut probe = (key.wrapping_mul(HASH_MULTIPLIER) >> (u64::BITS - slot_count.trailing_zeros())) as usize;
        loop {
            let number = self.slots[first_slot + probe];
            if number == EMPTY_SLOT {
                let number = self.handed_out[level_index];
                self.handed_out[level_index] += 1;
                self.slots[first_slot + probe] = number;
                self.first_tile[first_number + number as usize] = index as u16;
                return number;
            }
            let at = first_number + number as usize;
            if self.pattern_key(bitmap, level, self.first_tile[at] as usize) == key {
                self.repeated[at / u64::BITS as usize] |= 1 << (at % u64::BITS as usize);
                return number;
            }
            probe = (probe + 1) & (slot_count - 1);
        }
    }

    /// `tile`'s pattern number.
    #[inline]
    pub fn number(&self, tile: Tile) -> u16 {
        self.numbers.get(tile)
    }

    /// `tile`'s four children's pattern numbers, in reading order.
    #[inline]
    pub fn children_numbers(&self, tile: Tile) -> [u16; 4] {
        self.numbers.children(tile)
    }

    /// Whether another tile of `level` holds the pattern numbered
    /// `number` -- always, for a homogeneous one: only then can a tile
    /// with it be a copy.
    #[inline]
    pub fn repeats(&self, level: u8, number: u16) -> bool {
        let at = NUMBER_STARTS[level as usize] + number as usize;
        number < FIRST_PATTERN || self.repeated[at / u64::BITS as usize] >> (at % u64::BITS as usize) & 1 == 1
    }

    /// The first direction, near offsets before far, whose copy of
    /// `tile`, whose number is `number`, holds the same cells, if any.
    pub fn copy_source(&self, tile: Tile, number: u16) -> Option<(bool, u8)> {
        if !self.repeats(tile.level, number) {
            return None;
        }
        [false, true].into_iter().find_map(|far| {
            (0..crate::tile::DIRECTIONS)
                .find(|&direction| tile.offset_by(copy_offset(far, direction)).is_some_and(|source| self.number(source) == number))
                .map(|direction| (far, direction))
        })
    }
}
