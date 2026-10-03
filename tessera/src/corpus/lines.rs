//! Bitmaps drawn with lines: straight strokes of random length, run
//! horizontally, vertically or along either diagonal, at random widths
//! -- a width of one most often, each wider one less likely, as the line
//! set says. Like every corpus bitmap, settled by a seed.

use utilities::rng::Rng;
use bitmap::{Bitmap, WIDTH};

/// A kind of drawing: how many lines a bitmap is given, how wide they
/// get, and how many to measure over and to test.
pub struct LineSet {
    /// What a measurement calls it.
    pub name: &'static str,
    /// How many lines each bitmap is given.
    pub lines: u64,
    /// The widest a line is drawn, in cells.
    pub widest: i64,
    /// How likely a line is to be one wider again, in a hundred: at 50,
    /// each width is half as likely as the one before.
    pub wider_in_a_hundred: u64,
    /// How many to measure over.
    pub timed: u64,
    /// How many a test takes.
    pub tested: u64,
}

impl LineSet {
    /// `count` bitmaps of this set, built one at a time.
    pub fn take(&'static self, count: u64) -> Drawings {
        Drawings { seed: super::corpus_seed(), left: count, set: self }
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
    LineSet { name: "a few lines", lines: 8, widest: 16, wider_in_a_hundred: 50, timed: 12, tested: 2 },
    LineSet { name: "some lines", lines: 32, widest: 16, wider_in_a_hundred: 50, timed: 12, tested: 2 },
    LineSet { name: "many lines", lines: 128, widest: 16, wider_in_a_hundred: 50, timed: 12, tested: 2 },
];

/// The four ways a line runs: across, down, and down either diagonal.
const ORIENTATIONS: [(i64, i64); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];

/// A run of drawings from consecutive seeds, built one at a time.
pub struct Drawings {
    /// The next drawing's seed.
    seed: u64,
    /// How many drawings are still to come.
    left: u64,
    /// What every drawing is drawn by.
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
    let mut bitmap = Bitmap::new();
    let mut rng = Rng::new(seed);
    let side = WIDTH as u64;
    for _ in 0..set.lines {
        let (x, y) = (rng.below(side) as i64, rng.below(side) as i64);
        let (dx, dy) = ORIENTATIONS[rng.below(ORIENTATIONS.len() as u64) as usize];
        let length = 1 + rng.below(side) as i64;
        let mut width = 1;
        while width < set.widest && rng.percent_chance(set.wider_in_a_hundred) {
            width += 1;
        }
        for step in 0..length {
            let (step_x, step_y) = (x + dx * step, y + dy * step);
            bitmap.set_rect(step_x, step_y, step_x + width - 1, step_y + width - 1);
        }
    }
    bitmap
}
