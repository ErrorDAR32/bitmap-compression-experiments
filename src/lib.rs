//! A fixed-size 256x256 bit matrix backed by packed `u64` words.

mod mutate;
mod run_mesh;
pub use run_mesh::{Pick, Rect, RunMesh, Take};

pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 256;
const BITS_PER_WORD: usize = 64;
const WORDS: usize = (WIDTH * HEIGHT) / BITS_PER_WORD;

#[derive(Clone)]
pub struct BitMatrix {
    words: Box<[u64; WORDS]>,
}

impl BitMatrix {
    pub fn new() -> Self {
        Self {
            words: Box::new([0u64; WORDS]),
        }
    }

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

    pub fn set(&mut self, x: u8, y: u8) {
        let idx = Self::bit_index(x, y);
        self.words[idx / BITS_PER_WORD] |= 1u64 << (idx % BITS_PER_WORD);
    }

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

    pub fn count_set(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }
}

impl Default for BitMatrix {
    fn default() -> Self {
        Self::new()
    }
}

fn order(a: i64, b: i64) -> (i64, i64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

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
}
