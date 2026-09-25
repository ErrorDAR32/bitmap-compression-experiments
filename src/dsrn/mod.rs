//! Disjoint sized-tile region nesting: the homogeneity pyramid.
//!
//! DSRN describes a bitmap as a quadtree whose nodes are bound to a
//! tile size, each bound node emitting one value per tile. Every
//! decision it makes -- bind, defer, skip and subdivide -- is a
//! question about tiles: are the tiles of this region, at the size
//! this pass is working, each one all set or all clear?
//!
//! So the encoder asks that question constantly and nothing else
//! nearly as often, and the pyramid answers it in one lookup. For
//! every aligned square of side 2, 4, 8 and so on up to 256, it holds
//! two bits: whether that square is homogeneous, and if it is, what it
//! holds.
//!
//! It is built the way a mipmap is, each level from the one below
//! rather than from the cells: a square is homogeneous exactly when
//! its four quadrants are homogeneous and agree, which is five bit
//! operations on the level beneath. Levels are held as bit planes and
//! built a machine word at a time, so the whole pyramid costs a little
//! over one pass across the bitmap rather than one pass per level.

pub mod code;

use crate::data::bits::LINE_WORDS;
use crate::{BitMatrix, HEIGHT, WIDTH};

/// Tile sides, as powers of two: level `k` holds tiles of side
/// `1 << k`. Level 0 would be the cells themselves, which the bitmap
/// already is, so the pyramid starts at 1.
pub const LEVELS: usize = 8;

/// Words in each level's plane, largest level first. Level 1 is
/// 128x128 bits, and each level after it is a quarter of that.
const PLANE_WORDS: [usize; LEVELS + 1] = [0, 256, 64, 16, 4, 1, 1, 1, 1];

/// Where each level's plane begins, and how long the whole plane is.
const AT: [usize; LEVELS + 2] = {
    let mut at = [0; LEVELS + 2];
    let mut k = 1;
    while k <= LEVELS {
        at[k + 1] = at[k] + PLANE_WORDS[k];
        k += 1;
    }
    at
};

/// Every word of every level's two planes.
const WORDS: usize = AT[LEVELS + 1];

/// Whether each aligned square is homogeneous, and what it holds.
///
/// Built once per workspace and refilled per bitmap, like everything
/// else in the crate: the size is fixed by the matrix, so no bitmap
/// ever allocates.
pub struct Pyramid {
    /// A bit per square, set when every cell of it agrees.
    same: Box<[u64; WORDS]>,
    /// What a homogeneous square holds. Undefined where `same` is
    /// clear, which no reader looks at.
    held: Box<[u64; WORDS]>,
}

impl Default for Pyramid {
    fn default() -> Self {
        Self::new()
    }
}

impl Pyramid {
    /// An empty pyramid, with its room already found.
    pub fn new() -> Self {
        Self { same: Box::new([0; WORDS]), held: Box::new([0; WORDS]) }
    }

    /// The side of a level's plane, in tiles.
    pub(crate) const fn side(level: usize) -> usize {
        WIDTH >> level
    }

    /// Whether the square of side `1 << level` at tile coordinates
    /// `(x, y)` is homogeneous, and what it holds.
    pub fn at(&self, level: usize, x: usize, y: usize) -> Option<bool> {
        let bit = y * Self::side(level) + x;
        let word = AT[level] + bit / 64;
        if self.same[word] >> (bit % 64) & 1 == 0 {
            return None;
        }
        Some(self.held[word] >> (bit % 64) & 1 != 0)
    }

    /// Builds every level from the bitmap.
    pub fn rebuild(&mut self, bits: &BitMatrix) {
        self.first(bits);
        for level in 2..=LEVELS {
            self.step(level);
        }
    }

    /// Level 1, the 2x2 squares, read off the cells.
    ///
    /// Two rows are taken together: a square is all set where both rows
    /// have both of its bits, and all clear where neither row has
    /// either. That leaves an answer at every other bit position, which
    /// [`even_bits`] packs down.
    fn first(&mut self, bits: &BitMatrix) {
        let side = Self::side(1);
        for y in 0..side {
            let (top, bottom) = (bits.row(2 * y as u8), bits.row(2 * y as u8 + 1));
            for word in 0..LINE_WORDS {
                let both = top[word] & bottom[word];
                let either = top[word] | bottom[word];
                // A square is homogeneous when its two columns agree
                // with each other as well as its two rows.
                let ones = both & (both >> 1);
                let zeros = !either & (!either >> 1);

                let bit = y * side + word * 32;
                let (at, shift) = (AT[1] + bit / 64, bit % 64);
                self.same[at] |= (even_bits(ones | zeros) as u64) << shift;
                self.held[at] |= (even_bits(ones) as u64) << shift;
            }
        }
    }

    /// One level from the one below it. A square is homogeneous when
    /// its four quadrants are homogeneous and hold the same thing.
    ///
    /// A level narrower than a machine word packs several of its rows
    /// into one, so a row is read masked to its own width: without
    /// that, the quadrant test folds in the row beneath and squares
    /// come out homogeneous that are not.
    fn step(&mut self, level: usize) {
        let side = Self::side(level);
        for y in 0..side {
            let mut done = 0;
            while done < side {
                // Each word of the level below carries 32 squares of
                // this one, since a square is two of its tiles wide.
                let (same_top, held_top) = self.row_bits(level - 1, 2 * y, done * 2);
                let (same_bottom, held_bottom) = self.row_bits(level - 1, 2 * y + 1, done * 2);

                // All four quadrants homogeneous...
                let all_same = same_top & (same_top >> 1) & same_bottom & (same_bottom >> 1);
                // ...and all four agreeing with the upper left one.
                let agree = !(held_top ^ (held_top >> 1))
                    & !(held_top ^ held_bottom)
                    & !(held_top ^ (held_bottom >> 1));

                let take = (side - done).min(32);
                let mask = if take == 64 { u64::MAX } else { (1u64 << take) - 1 };
                let bit = y * side + done;
                let (at, shift) = (AT[level] + bit / 64, bit % 64);
                self.same[at] |= ((even_bits(all_same & agree) as u64) & mask) << shift;
                self.held[at] |= ((even_bits(held_top) as u64) & mask) << shift;
                done += take;
            }
        }
    }

    /// One row of a level, from `from` onwards, masked to the row's own
    /// width and to what is left of the word it starts in.
    ///
    /// A row never straddles a word: every level's side is either a
    /// multiple of 64 or a power of two that divides it.
    fn row_bits(&self, level: usize, row: usize, from: usize) -> (u64, u64) {
        let width = Self::side(level);
        let bit = row * width + from;
        let (at, shift) = (AT[level] + bit / 64, bit % 64);
        let take = (width - from).min(64 - shift);
        let mask = if take == 64 { u64::MAX } else { (1u64 << take) - 1 };
        ((self.same[at] >> shift) & mask, (self.held[at] >> shift) & mask)
    }

    /// Clears every level, so a workspace can take the next bitmap.
    pub fn clear(&mut self) {
        self.same.fill(0);
        self.held.fill(0);
    }

    /// Whether every square of a `s` by `s` block of them is
    /// homogeneous, and whether any is.
    ///
    /// One question per node per pass, and the pass asks it of every
    /// node, so it is answered over the plane's words rather than
    /// square by square: a block of 128 squares is two loads.
    pub fn block(&self, level: usize, tx: usize, ty: usize, s: usize) -> (bool, bool) {
        let (mut all, mut any) = (true, false);
        for row in ty..ty + s {
            let mut done = 0;
            while done < s {
                let take = (s - done).min(64);
                let span = self.span(level, row, tx + done, take);
                let want = if take == 64 { u64::MAX } else { (1u64 << take) - 1 };
                all &= span == want;
                any |= span != 0;
                done += take;
            }
        }
        (all, any)
    }

    /// `take` homogeneity bits of one row, starting at `from`.
    ///
    /// Never straddles a word: a block of `s` squares starts on a
    /// multiple of `s`, and every level's side is either a multiple of
    /// 64 or a power of two that divides it.
    fn span(&self, level: usize, row: usize, from: usize, take: usize) -> u64 {
        let bit = row * Self::side(level) + from;
        let (at, shift) = (AT[level] + bit / 64, bit % 64);
        let mask = if take == 64 { u64::MAX } else { (1u64 << take) - 1 };
        (self.same[at] >> shift) & mask
    }

    /// What a homogeneous square holds, without asking again whether
    /// it is homogeneous.
    pub fn value(&self, level: usize, x: usize, y: usize) -> bool {
        let bit = y * Self::side(level) + x;
        self.held[AT[level] + bit / 64] >> (bit % 64) & 1 != 0
    }
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

/// For one tile size, whether a region's tiles are all homogeneous
/// and whether any of them is.
///
/// This is the question every pass asks of every region, and asking it
/// by scanning the region's tiles asks it again at every step of a
/// descent: a region that subdivides hands its four children exactly
/// the tiles it just read, and they read them again between them.
///
/// It does not need scanning, because it recurs. A region's tiles at
/// one size are its four children's tiles at that size, all of them
/// and nothing else, so
///
/// ```text
///     all(region) = all(top left) & all(top right)
///                 & all(bottom left) & all(bottom right)
///     any(region) = any(top left) | any(top right)
///                 | any(bottom left) | any(bottom right)
/// ```
///
/// and a region of exactly the tile's size is one tile, which the
/// pyramid already answers. So one fold upwards per pass settles every
/// region at once, four tests each, and the answer is then a bit. It
/// is the same answer as scanning, not an approximation of it.
pub struct Folds {
    all: Box<[u64; WORDS]>,
    any: Box<[u64; WORDS]>,
}

impl Default for Folds {
    fn default() -> Self {
        Self::new()
    }
}

impl Folds {
    /// Empty folds, with their room already found.
    pub fn new() -> Self {
        Self { all: Box::new([0; WORDS]), any: Box::new([0; WORDS]) }
    }

    /// Folds the pyramid's homogeneity plane for one tile size, from
    /// regions of that size up to the whole bitmap.
    pub fn rebuild(&mut self, pyramid: &Pyramid, tile: usize) {
        // A region of exactly the tile's size is one tile.
        let (at, end) = (AT[tile], AT[tile] + PLANE_WORDS[tile]);
        self.all[at..end].copy_from_slice(&pyramid.same[at..end]);
        self.any[at..end].copy_from_slice(&pyramid.same[at..end]);

        for level in tile + 1..=LEVELS {
            let side = Pyramid::side(level);
            let (at, end) = (AT[level], AT[level] + PLANE_WORDS[level]);
            self.all[at..end].fill(0);
            self.any[at..end].fill(0);

            for y in 0..side {
                let mut done = 0;
                while done < side {
                    let (all_top, any_top) = self.rows(level - 1, 2 * y, done * 2);
                    let (all_bottom, any_bottom) = self.rows(level - 1, 2 * y + 1, done * 2);
                    let all = all_top & (all_top >> 1) & all_bottom & (all_bottom >> 1);
                    let any = any_top | (any_top >> 1) | any_bottom | (any_bottom >> 1);

                    let take = (side - done).min(32);
                    let mask = if take == 64 { u64::MAX } else { (1u64 << take) - 1 };
                    let bit = y * side + done;
                    let (word, shift) = (AT[level] + bit / 64, bit % 64);
                    self.all[word] |= ((even_bits(all) as u64) & mask) << shift;
                    self.any[word] |= ((even_bits(any) as u64) & mask) << shift;
                    done += take;
                }
            }
        }
    }

    /// One row of both folds, masked to the row's own width. A level
    /// narrower than a word packs several of its rows into one.
    fn rows(&self, level: usize, row: usize, from: usize) -> (u64, u64) {
        let width = Pyramid::side(level);
        let bit = row * width + from;
        let (at, shift) = (AT[level] + bit / 64, bit % 64);
        let take = (width - from).min(64 - shift);
        let mask = if take == 64 { u64::MAX } else { (1u64 << take) - 1 };
        ((self.all[at] >> shift) & mask, (self.any[at] >> shift) & mask)
    }

    /// Whether the region's tiles are all homogeneous, and whether any
    /// of them is.
    pub fn at(&self, level: usize, x: usize, y: usize) -> (bool, bool) {
        let bit = y * Pyramid::side(level) + x;
        let (word, shift) = (AT[level] + bit / 64, bit % 64);
        (self.all[word] >> shift & 1 != 0, self.any[word] >> shift & 1 != 0)
    }
}

/// Everything the pyramid says, worked out from the cells instead.
///
/// Slow, obviously right, and what the fast path is checked against.
#[doc(hidden)]
pub fn homogeneous_by_reading(bits: &BitMatrix, level: usize, x: usize, y: usize) -> Option<bool> {
    let side = 1usize << level;
    let (x0, y0) = (x * side, y * side);
    let first = bits.get(x0 as u8, y0 as u8);
    for dy in 0..side {
        for dx in 0..side {
            if x0 + dx >= WIDTH || y0 + dy >= HEIGHT {
                continue;
            }
            if bits.get((x0 + dx) as u8, (y0 + dy) as u8) != first {
                return None;
            }
        }
    }
    Some(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples;

    /// Folding up from the children has to answer what scanning the
    /// tiles answers, for every region of every size at every tile
    /// size. The whole point of the fold is that it is the same
    /// question, so this is what says it is.
    #[test]
    fn folding_from_the_children_agrees_with_scanning_the_tiles() {
        let (mut pyramid, mut folds) = (Pyramid::new(), Folds::new());
        let mut cases = vec![BitMatrix::new()];
        for shape in samples::SHAPES {
            cases.extend(shape.tested());
        }
        cases.extend(samples::grown(0, 1.0, 0.0, 1));

        for bits in &cases {
            pyramid.clear();
            pyramid.rebuild(bits);
            for tile in 1..=LEVELS {
                folds.rebuild(&pyramid, tile);
                for level in tile..=LEVELS {
                    let side = Pyramid::side(level);
                    let across = 1 << (level - tile);
                    for y in 0..side {
                        for x in 0..side {
                            assert_eq!(
                                folds.at(level, x, y),
                                pyramid.block(tile, x * across, y * across, across),
                                "tile {tile}, region {level} at ({x}, {y})"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Every square of every level, against reading the cells.
    #[test]
    fn the_pyramid_agrees_with_reading_the_cells() {
        let mut pyramid = Pyramid::new();
        let mut cases = vec![BitMatrix::new()];
        for shape in samples::SHAPES {
            cases.extend(shape.tested());
        }
        cases.extend(samples::grown(0, 1.0, 0.0, 1));

        for bits in &cases {
            pyramid.clear();
            pyramid.rebuild(bits);
            for level in 1..=LEVELS {
                let side = Pyramid::side(level);
                for y in 0..side {
                    for x in 0..side {
                        assert_eq!(
                            pyramid.at(level, x, y),
                            homogeneous_by_reading(bits, level, x, y),
                            "level {level} square ({x}, {y})"
                        );
                    }
                }
            }
        }
    }
}
