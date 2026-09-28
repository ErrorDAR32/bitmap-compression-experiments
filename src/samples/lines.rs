//! Bitmaps drawn with lines: straight strokes of random length, run
//! horizontally, vertically or along either diagonal, at random widths
//! -- a width of one most often, and each wider one half as likely as
//! the one before. Like every sample, settled by a seed.

use super::rolls::Rolls;
use crate::{Bitmap, WIDTH};

/// A kind of drawing: how many lines a bitmap is given, and how many to
/// measure over and to test.
pub struct LineSet {
    pub name: &'static str,
    pub lines: u64,
    pub timed: u64,
    pub tested: u64,
}

impl LineSet {
    /// `count` bitmaps of this set, built one at a time.
    pub fn take(&'static self, count: u64) -> Drawings {
        Drawings { seed: super::sample_seed(self.name), left: count, set: self }
    }

    /// As many as a timed run of this set should take.
    pub fn timed(&'static self) -> Drawings {
        self.take(self.timed)
    }

    /// As many as a unit test of this set should take.
    pub fn tested(&'static self) -> Drawings {
        self.take(self.tested)
    }
}

/// From a few strokes to a tangle.
pub const LINE_SETS: [LineSet; 3] = [
    LineSet { name: "a few lines", lines: 8, timed: 12, tested: 2 },
    LineSet { name: "some lines", lines: 32, timed: 12, tested: 2 },
    LineSet { name: "many lines", lines: 128, timed: 12, tested: 2 },
];

/// The widest a line is drawn: a sixteenth of the bitmap. Wider is a
/// block, not a line -- and at half as likely a step, never reached in
/// practice anyway.
const WIDEST: i64 = 16;

/// How likely a line is to be one wider again, in a hundred: each width
/// half as likely as the one before.
const WIDER_IN_A_HUNDRED: u64 = 50;

/// The four ways a line runs: across, down, and down either diagonal.
const ORIENTATIONS: [(i64, i64); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];

/// A run of drawings from consecutive seeds, built one at a time.
pub struct Drawings {
    seed: u64,
    left: u64,
    set: &'static LineSet,
}

impl Iterator for Drawings {
    type Item = Bitmap;

    fn next(&mut self) -> Option<Bitmap> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        self.seed += 1;
        Some(one_drawn(self.seed - 1, self.set))
    }
}

/// One drawing: `set.lines` lines, each from a random cell, in a random
/// orientation, of a random length up to the bitmap's side, at a random
/// width. The border cuts what runs past it.
pub fn one_drawn(seed: u64, set: &LineSet) -> Bitmap {
    let mut bits = Bitmap::new();
    let mut rolls = Rolls(seed);
    let side = WIDTH as u64;
    for _ in 0..set.lines {
        let (x, y) = (rolls.upto(side) as i64, rolls.upto(side) as i64);
        let (dx, dy) = ORIENTATIONS[rolls.upto(ORIENTATIONS.len() as u64) as usize];
        let length = 1 + rolls.upto(side) as i64;
        let mut width = 1;
        while width < WIDEST && rolls.chance(WIDER_IN_A_HUNDRED) {
            width += 1;
        }
        for step in 0..length {
            let (cx, cy) = (x + dx * step, y + dy * step);
            bits.set_rect(cx, cy, cx + width - 1, cy + width - 1);
        }
    }
    bits
}
