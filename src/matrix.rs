//! The bitmap itself: 256 by 256 bits, packed into machine words.
//!
//! Row-major, four `u64` to a row, least significant bit leftmost, so
//! cell `(x, y)` is bit `x % 64` of word `y * 4 + x / 64`. Every reader
//! in the crate relies on that layout: [`crate::mesh`] copies a row
//! straight out of it, and [`crate::bits`] holds the word operations
//! that read a row as runs.
//!
//! The drawing methods take `i64` and clamp, so a caller can ask for a
//! circle hanging off the edge without doing the arithmetic first.

use crate::{BITS_PER_WORD, HEIGHT, WIDTH, WORDS};

/// 65536 bits, boxed so that passing one around moves a pointer rather
/// than eight kilobytes.
#[derive(Clone)]
pub struct BitMatrix {
    words: Box<[u64; WORDS]>,
}

impl BitMatrix {
    /// An empty matrix, with nothing set.
    pub fn new() -> Self {
        Self {
            words: Box::new([0u64; WORDS]),
        }
    }

    /// Where a cell's bit sits, counting from the top-left across each
    /// row in turn. The word is this divided by 64 and the bit within
    /// it the remainder, which is the whole of the layout.
    fn bit_index(x: u8, y: u8) -> usize {
        y as usize * WIDTH + x as usize
    }

    /// `x` and `y` are `u8`, so every value from 0 to 255 is a valid
    /// coordinate in this 256x256 matrix and out-of-bounds access is
    /// impossible to express, not just checked at runtime.
    pub fn get(&self, x: u8, y: u8) -> bool {
        let idx = Self::bit_index(x, y);
        (self.words[idx / BITS_PER_WORD] >> (idx % BITS_PER_WORD)) & 1 == 1
    }

    /// Stands the cell up. Already standing is not an error.
    pub fn set(&mut self, x: u8, y: u8) {
        let idx = Self::bit_index(x, y);
        self.words[idx / BITS_PER_WORD] |= 1u64 << (idx % BITS_PER_WORD);
    }

    /// Takes the cell away. Already gone is not an error.
    pub fn unset(&mut self, x: u8, y: u8) {
        let idx = Self::bit_index(x, y);
        self.words[idx / BITS_PER_WORD] &= !(1u64 << (idx % BITS_PER_WORD));
    }

    /// Clears every bit back to 0.
    pub fn reset(&mut self) {
        for word in self.words.iter_mut() {
            *word = 0;
        }
    }

    /// Sets every bit contained in the inclusive rectangle described by
    /// the two corner points, in any order. Coordinates are clamped to
    /// the matrix bounds.
    pub fn set_rect(&mut self, x0: i64, y0: i64, x1: i64, y1: i64) {
        self.for_each_in_rect(x0, y0, x1, y1, |m, x, y| m.set(x, y));
    }

    /// Unsets every bit contained in the inclusive rectangle described by
    /// the two corner points, in any order. Coordinates are clamped to
    /// the matrix bounds.
    pub fn unset_rect(&mut self, x0: i64, y0: i64, x1: i64, y1: i64) {
        self.for_each_in_rect(x0, y0, x1, y1, |m, x, y| m.unset(x, y));
    }

    /// Visits every cell of a rectangle given in any order and in
    /// coordinates that may lie outside the matrix, which is what lets
    /// the drawing methods take `i64` and clamp. Shared by setting and
    /// unsetting so the two cannot disagree about what a rectangle is.
    fn for_each_in_rect(
        &mut self,
        x0: i64,
        y0: i64,
        x1: i64,
        y1: i64,
        op: impl Fn(&mut Self, u8, u8),
    ) {
        let (lo_x, hi_x) = order(x0, x1);
        let (lo_y, hi_y) = order(y0, y1);
        let lo_x = clamp(lo_x, 0, WIDTH as i64 - 1) as u8;
        let hi_x = clamp(hi_x, 0, WIDTH as i64 - 1) as u8;
        let lo_y = clamp(lo_y, 0, HEIGHT as i64 - 1) as u8;
        let hi_y = clamp(hi_y, 0, HEIGHT as i64 - 1) as u8;

        for y in lo_y..=hi_y {
            for x in lo_x..=hi_x {
                op(self, x, y);
            }
        }
    }

    /// Sets every bit whose center lies within `radius` of `(cx, cy)`,
    /// approximating a filled circle by testing squared distance.
    pub fn set_circle(&mut self, cx: i64, cy: i64, radius: i64) {
        self.for_each_in_circle(cx, cy, radius, |m, x, y| m.set(x, y));
    }

    /// Unsets every bit whose center lies within `radius` of `(cx, cy)`.
    pub fn unset_circle(&mut self, cx: i64, cy: i64, radius: i64) {
        self.for_each_in_circle(cx, cy, radius, |m, x, y| m.unset(x, y));
    }

    /// Visits every cell whose centre lies within `radius` of
    /// `(cx, cy)`, by walking the bounding box and testing squared
    /// distance, so no square root is taken and nothing is approximated
    /// beyond the pixel grid itself. A negative radius draws nothing.
    fn for_each_in_circle(
        &mut self,
        cx: i64,
        cy: i64,
        radius: i64,
        op: impl Fn(&mut Self, u8, u8),
    ) {
        if radius < 0 {
            return;
        }
        let r2 = radius * radius;
        let lo_x = clamp(cx - radius, 0, WIDTH as i64 - 1) as u8;
        let hi_x = clamp(cx + radius, 0, WIDTH as i64 - 1) as u8;
        let lo_y = clamp(cy - radius, 0, HEIGHT as i64 - 1) as u8;
        let hi_y = clamp(cy + radius, 0, HEIGHT as i64 - 1) as u8;

        for y in lo_y..=hi_y {
            let dy = y as i64 - cy;
            for x in lo_x..=hi_x {
                let dx = x as i64 - cx;
                if dx * dx + dy * dy <= r2 {
                    op(self, x, y);
                }
            }
        }
    }

    /// A bitmap grown from a seed, so that the same three arguments
    /// always give the same bitmap.
    ///
    /// `density` is the share of the 65536 cells that end up set.
    /// `cluster` is how often a new cell lands beside one already set
    /// rather than anywhere at all: at 0 the set cells are scattered,
    /// and every one of them is its own rectangle; at 1 they only ever
    /// extend what is already standing, so the bitmap comes out as a
    /// few solid blobs. Everything interesting is in between, and the
    /// two parameters cover between them the cases that were reached
    /// before by hand -- the checkerboard is density 0.5 and cluster 0,
    /// a solid square is density 1.
    ///
    /// Cells beside the ones already set are kept with repeats, so a
    /// cell with three set neighbours is three times as likely to be
    /// taken as one with a single neighbour. That is the point: it is
    /// what makes a blob fill in rather than sprawl.
    #[doc(hidden)]
    pub fn grown(seed: u64, density: f64, cluster: f64) -> Self {
        let wanted = (density.clamp(0.0, 1.0) * (WIDTH * HEIGHT) as f64) as usize;
        let cluster = (cluster.clamp(0.0, 1.0) * u32::MAX as f64) as u64;

        let mut bits = Self::new();
        // Xorshift needs a state that is not zero, and it is the seed
        // alone that has to reproduce the bitmap.
        let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        // Unset cells beside a set one, with repeats.
        let mut edge: Vec<(u8, u8)> = Vec::new();
        let mut standing = 0;

        while standing < wanted {
            let beside = (!edge.is_empty() && next() % (1 << 32) < cluster)
                .then(|| {
                    while let Some(at) = (!edge.is_empty()).then(|| next() as usize % edge.len()) {
                        let cell = edge.swap_remove(at);
                        if !bits.get(cell.0, cell.1) {
                            return Some(cell);
                        }
                    }
                    None
                })
                .flatten();

            let (x, y) = match beside {
                Some(cell) => cell,
                None => bits.anywhere_clear(&mut next),
            };

            bits.set(x, y);
            standing += 1;
            let (x, y) = (x as i32, y as i32);
            for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                if (0..WIDTH as i32).contains(&nx) && (0..HEIGHT as i32).contains(&ny) {
                    let (nx, ny) = (nx as u8, ny as u8);
                    if !bits.get(nx, ny) {
                        edge.push((nx, ny));
                    }
                }
            }
        }

        bits
    }

    /// Any cell still clear, found by guessing and then, once guessing
    /// stops paying, by looking.
    fn anywhere_clear(&self, next: &mut impl FnMut() -> u64) -> (u8, u8) {
        for _ in 0..64 {
            let roll = next();
            let (x, y) = (roll as u8, (roll >> 8) as u8);
            if !self.get(x, y) {
                return (x, y);
            }
        }

        let word = self
            .words
            .iter()
            .position(|w| *w != u64::MAX)
            .expect("a bitmap that is not already full");
        let at = word * BITS_PER_WORD + (!self.words[word]).trailing_zeros() as usize;
        ((at % WIDTH) as u8, (at / WIDTH) as u8)
    }

    /// How many cells are standing, counted a word at a time.
    pub fn count_set(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }

    /// Splits the set cells into those with no orthogonal neighbour and
    /// the rest.
    ///
    /// A cell standing alone is its own rectangle in every partition,
    /// and nothing can ever be laid against it: a rectangle touching one
    /// of its faces would have to contain a cell beside it, and there is
    /// none. So it decides nothing and nothing decides it, and the work
    /// of meshing and rewriting can pass it by entirely.
    ///
    /// Found a row of words at a time. Whether the cell to the left is
    /// set is the row shifted up one bit, carrying across the word
    /// boundary; above and below are the neighbouring rows unshifted.
    #[doc(hidden)]
    pub fn split_isolated(&self) -> (Self, Self) {
        let (mut alone, mut rest) = (Self::new(), Self::new());
        self.split_isolated_into(&mut alone, &mut rest);
        (alone, rest)
    }

    /// The same, into bitmaps that already exist. Whatever they held is
    /// overwritten.
    pub(crate) fn split_isolated_into(&self, alone: &mut Self, rest: &mut Self) {
        const PER_ROW: usize = WIDTH / BITS_PER_WORD;

        alone.words.fill(0);
        rest.words.copy_from_slice(&*self.words);
        for y in 0..HEIGHT {
            let row = y * PER_ROW;
            for i in 0..PER_ROW {
                let word = self.words[row + i];
                if word == 0 {
                    continue;
                }

                let left = (word << 1) | if i > 0 { self.words[row + i - 1] >> 63 } else { 0 };
                let right =
                    (word >> 1) | if i + 1 < PER_ROW { self.words[row + i + 1] << 63 } else { 0 };
                let above = if y > 0 { self.words[row - PER_ROW + i] } else { 0 };
                let below = if y + 1 < HEIGHT { self.words[row + PER_ROW + i] } else { 0 };

                let solo = word & !(left | right | above | below);
                alone.words[row + i] = solo;
                rest.words[row + i] = word & !solo;
            }
        }
    }

    /// The machine words holding one row.
    pub(crate) fn row(&self, y: u8) -> &[u64] {
        const PER_ROW: usize = WIDTH / BITS_PER_WORD;
        let at = y as usize * PER_ROW;
        &self.words[at..at + PER_ROW]
    }

    /// Calls `visit` with every set cell, in scan order.
    pub(crate) fn for_each_set(&self, mut visit: impl FnMut(u8, u8)) {
        for (index, &word) in self.words.iter().enumerate() {
            let mut bits = word;
            while bits != 0 {
                let at = index * BITS_PER_WORD + bits.trailing_zeros() as usize;
                visit((at % WIDTH) as u8, (at / WIDTH) as u8);
                bits &= bits - 1;
            }
        }
    }
}

impl Default for BitMatrix {
    /// The same as [`BitMatrix::new`]: empty.
    fn default() -> Self {
        Self::new()
    }
}

/// The two given either way round, smaller first, so that a caller can
/// name a rectangle by any two opposite corners.
fn order(a: i64, b: i64) -> (i64, i64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// `v` pulled into `lo..=hi`. Written out rather than `i64::clamp` so
/// that the intent is visible next to the coordinate arithmetic it
/// serves.
fn clamp(v: i64, lo: i64, hi: i64) -> i64 {
    v.max(lo).min(hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_empty() {
        let m = BitMatrix::new();
        assert_eq!(m.count_set(), 0);
        assert!(!m.get(0, 0));
        assert!(!m.get(255, 255));
    }

    #[test]
    fn set_and_unset_single_bit() {
        let mut m = BitMatrix::new();
        m.set(10, 20);
        assert!(m.get(10, 20));
        assert_eq!(m.count_set(), 1);
        m.unset(10, 20);
        assert!(!m.get(10, 20));
        assert_eq!(m.count_set(), 0);
    }

    #[test]
    fn rect_is_inclusive_and_order_independent() {
        let mut m = BitMatrix::new();
        m.set_rect(5, 5, 2, 2);
        assert_eq!(m.count_set(), 16); // 4x4 inclusive area
        for y in 2..=5 {
            for x in 2..=5 {
                assert!(m.get(x, y));
            }
        }
        m.unset_rect(2, 2, 5, 5);
        assert_eq!(m.count_set(), 0);
    }

    #[test]
    fn rect_clamps_to_bounds() {
        let mut m = BitMatrix::new();
        m.set_rect(-10, -10, 1, 1);
        assert_eq!(m.count_set(), 4);
    }

    #[test]
    fn circle_includes_center_and_excludes_far_corners() {
        let mut m = BitMatrix::new();
        m.set_circle(128, 128, 5);
        assert!(m.get(128, 128));
        assert!(m.get(133, 128));
        assert!(!m.get(134, 128));
        // corner of the bounding box should be excluded by the distance test
        assert!(!m.get(133, 133));
    }

    #[test]
    fn unset_circle_clears_previously_set_bits() {
        let mut m = BitMatrix::new();
        m.set_circle(50, 50, 10);
        let before = m.count_set();
        assert!(before > 0);
        m.unset_circle(50, 50, 10);
        assert_eq!(m.count_set(), 0);
    }

    #[test]
    fn reset_clears_everything() {
        let mut m = BitMatrix::new();
        m.set_rect(0, 0, 255, 255);
        assert_eq!(m.count_set() as usize, WIDTH * HEIGHT);
        m.reset();
        assert_eq!(m.count_set(), 0);
    }

    /// The same seed has to give the same bitmap, and a different one a
    /// different bitmap, or a reproduction is not a reproduction.
    #[test]
    fn growing_is_settled_by_its_seed() {
        for (density, cluster) in [(0.05, 0.0), (0.2, 0.5), (0.4, 0.9), (0.9, 0.95)] {
            let once = BitMatrix::grown(12345, density, cluster);
            let again = BitMatrix::grown(12345, density, cluster);
            assert_eq!(once.words, again.words, "the same seed wandered");

            let other = BitMatrix::grown(12346, density, cluster);
            assert_ne!(once.words, other.words, "two seeds agreed");

            let wanted = (density * 65536.0) as u32;
            assert_eq!(once.count_set(), wanted, "the density is not the density");
        }
    }

    /// Clustering has to do what it says: at nothing the set cells are
    /// scattered and hardly any of them touch, at almost everything
    /// they are in blobs and nearly all of them do.
    #[test]
    fn clustering_decides_how_much_touches() {
        let touching = |bits: &BitMatrix| {
            let mut with = 0;
            for y in 0..=u8::MAX {
                for x in 0..=u8::MAX {
                    if bits.get(x, y) {
                        let (x, y) = (x as i32, y as i32);
                        let near = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                            .into_iter()
                            .filter(|(nx, ny)| (0..256).contains(nx) && (0..256).contains(ny))
                            .any(|(nx, ny)| bits.get(nx as u8, ny as u8));
                        with += u32::from(near);
                    }
                }
            }
            with as f64 / bits.count_set() as f64
        };

        let scattered = touching(&BitMatrix::grown(7, 0.1, 0.0));
        let blobs = touching(&BitMatrix::grown(7, 0.1, 0.99));
        assert!(scattered < 0.45, "scattered cells touch too much: {scattered}");
        assert!(blobs > 0.95, "clustered cells touch too little: {blobs}");
        assert!(blobs > scattered * 2.0, "clustering made no difference");
    }
}
