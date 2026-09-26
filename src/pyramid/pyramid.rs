//! Building the pyramid, the way a mipmap is built: each level from
//! the one finer than it rather than from the cells.
//!
//! A tile is homogeneous exactly when its four children are
//! homogeneous and agree, which is five bit operations on the level
//! below. Levels are bit planes and are built a machine word at a
//! time, so the whole pyramid costs a little over one pass across the
//! bitmap rather than one pass per level.

use super::pyramid_data::{
    tile_side, tiles_across, Pyramid, CELL_LEVEL, DIRECTIONS, FINEST_LEVEL_HELD,
    PYRAMID_LEVEL_BOUNDARIES,
};
use crate::bitmap::bitmap_words::LINE_WORDS;
use crate::Bitmap;

impl Pyramid {
    /// Builds every level from the bitmap, finest first.
    pub fn rebuild(&mut self, bitmap: &Bitmap) {
        self.finest_level_from_the_cells(bitmap);
        for level in (0..FINEST_LEVEL_HELD).rev() {
            self.level_from_the_one_below(level);
        }
        for level in 0..=FINEST_LEVEL_HELD {
            self.copyable_level(bitmap, level);
        }
    }

    /// Which tiles of a level hold the same cells as a neighbour
    /// reading order puts before them.
    ///
    /// This one does not fold. A tile matching the tile beside it
    /// says nothing about whether the tile above them both matches
    /// the one beside that, because at the level above they are two
    /// tiles apart, not one. So it is read off the cells -- but a
    /// row of a tile is a run of whole words, or a run inside one
    /// word, so a row is one comparison rather than one a cell, and
    /// the first row that differs ends it.
    fn copyable_level(&mut self, bitmap: &Bitmap, level: usize) {
        let (across, side) = (tiles_across(level), tile_side(level));
        for y in 0..across {
            for x in 0..across {
                let copyable = DIRECTIONS.iter().any(|&(dx, dy)| {
                    let (at_x, at_y) = (x as isize + dx, y as isize + dy);
                    at_x >= 0
                        && at_y >= 0
                        && at_x < across as isize
                        && same_tiles(bitmap, side, (x, y), (at_x as usize, at_y as usize))
                });
                if copyable {
                    let (word, shift) = Self::bit_of_tile(level, x, y);
                    self.copyable_tiles[word] |= 1 << shift;
                }
            }
        }
    }

    /// The 2x2 tiles, read off the cells.
    ///
    /// Two rows are taken together: a tile is all set where both rows
    /// have both of its bits, and all clear where neither row has
    /// either. That leaves an answer at every other bit position,
    /// which [`even_bits`] packs down.
    fn finest_level_from_the_cells(&mut self, bitmap: &Bitmap) {
        let across = tiles_across(FINEST_LEVEL_HELD);
        for y in 0..across {
            let (top, bottom) = (bitmap.row(2 * y as u8), bitmap.row(2 * y as u8 + 1));
            for word in 0..LINE_WORDS {
                let both = top[word] & bottom[word];
                let either = top[word] | bottom[word];
                // A tile is homogeneous when its two columns agree
                // with each other as well as its two rows.
                let ones = both & (both >> 1);
                let zeros = !either & (!either >> 1);

                let bit = y * across + word * 32;
                let at = PYRAMID_LEVEL_BOUNDARIES[FINEST_LEVEL_HELD] + bit / 64;
                let shift = bit % 64;
                self.homogeneous_tiles[at] |= (even_bits(ones | zeros) as u64) << shift;
                self.homogeneous_tile_values[at] |= (even_bits(ones) as u64) << shift;
            }
        }
    }

    /// One level from the one below it.
    ///
    /// A level narrower than a machine word packs several of its rows
    /// into one word, so a row is read masked to its own width.
    /// Without that the four-children test folds in the row beneath
    /// and tiles come out homogeneous that are not.
    fn level_from_the_one_below(&mut self, level: usize) {
        let across = tiles_across(level);
        for y in 0..across {
            let mut done = 0;
            while done < across {
                // Each word of the level below carries 32 tiles of
                // this one, since a tile is two of its children wide.
                let (top_same, top_value) = self.row_of_a_level(level + 1, 2 * y, done * 2);
                let (low_same, low_value) = self.row_of_a_level(level + 1, 2 * y + 1, done * 2);

                // All four children homogeneous...
                let all_homogeneous = top_same & (top_same >> 1) & low_same & (low_same >> 1);
                // ...and all four agreeing with the top left one.
                let all_agree = !(top_value ^ (top_value >> 1))
                    & !(top_value ^ low_value)
                    & !(top_value ^ (low_value >> 1));

                let take = (across - done).min(32);
                let mask = (1u64 << take) - 1;
                let bit = y * across + done;
                let at = PYRAMID_LEVEL_BOUNDARIES[level] + bit / 64;
                let shift = bit % 64;
                self.homogeneous_tiles[at] |=
                    ((even_bits(all_homogeneous & all_agree) as u64) & mask) << shift;
                self.homogeneous_tile_values[at] |=
                    ((even_bits(top_value) as u64) & mask) << shift;
                done += take;
            }
        }
    }

    /// One row of a level, from `from` onwards, masked to the row's
    /// own width and to what is left of the word it starts in.
    ///
    /// A row never straddles a word: every level's width across is
    /// either a multiple of 64 or a power of two that divides it.
    fn row_of_a_level(&self, level: usize, row: usize, from: usize) -> (u64, u64) {
        let across = tiles_across(level);
        let bit = row * across + from;
        let (at, shift) = (PYRAMID_LEVEL_BOUNDARIES[level] + bit / 64, bit % 64);
        let take = (across - from).min(64 - shift);
        let mask = if take == 64 { u64::MAX } else { (1u64 << take) - 1 };
        (
            (self.homogeneous_tiles[at] >> shift) & mask,
            (self.homogeneous_tile_values[at] >> shift) & mask,
        )
    }
}

/// Whether two tiles of the same size hold the same cells, read a
/// word of a row at a time.
///
/// A tile's row is a run of `side` bits starting at a multiple of
/// `side`, so it is either whole words or a run inside one word, and
/// never straddles two.
fn same_tiles(bitmap: &Bitmap, side: usize, a: (usize, usize), b: (usize, usize)) -> bool {
    let (a_x, b_x) = (a.0 * side, b.0 * side);
    for row in 0..side {
        let mine = bitmap.row((a.1 * side + row) as u8);
        let theirs = bitmap.row((b.1 * side + row) as u8);
        if !same_run(mine, theirs, a_x, b_x, side) {
            return false;
        }
    }
    true
}

/// Whether two runs of `side` bits of two rows agree.
fn same_run(mine: &[u64], theirs: &[u64], a_x: usize, b_x: usize, side: usize) -> bool {
    if side >= 64 {
        let words = side / 64;
        return (0..words).all(|word| mine[a_x / 64 + word] == theirs[b_x / 64 + word]);
    }
    let mask = (1u64 << side) - 1;
    (mine[a_x / 64] >> (a_x % 64)) & mask == (theirs[b_x / 64] >> (b_x % 64)) & mask
}

/// What a tile holds, taking the cells as level [`CELL_LEVEL`].
///
/// The pyramid does not hold the cells, so a caller that walks down to
/// them needs somewhere to ask. This is that somewhere, and it is the
/// only place in the crate that knows the pyramid stops one level
/// short.
pub fn tile_of_bitmap(pyramid: &Pyramid, bitmap: &Bitmap, level: usize, x: usize, y: usize) -> Option<bool> {
    if level == CELL_LEVEL {
        return Some(bitmap.get(x as u8, y as u8));
    }
    pyramid.tile(level, x, y)
}

/// The bits at even positions, packed down to the bottom half.
///
/// The standard halving gather: pairs, then nybbles, then bytes, and
/// so on, each step folding the gaps out.
fn even_bits(mut x: u64) -> u32 {
    x &= 0x5555_5555_5555_5555;
    x = (x | (x >> 1)) & 0x3333_3333_3333_3333;
    x = (x | (x >> 2)) & 0x0f0f_0f0f_0f0f_0f0f;
    x = (x | (x >> 4)) & 0x00ff_00ff_00ff_00ff;
    x = (x | (x >> 8)) & 0x0000_ffff_0000_ffff;
    x = (x | (x >> 16)) & 0x0000_0000_ffff_ffff;
    x as u32
}
