//! The one source of test bitmaps, and the shapes worth measuring on.
//!
//! Nothing in this crate is measured on a bitmap anybody drew by hand.
//! Every sample comes from here, tests and measurements alike, so
//! changing what anything runs on is a change in one file. The one
//! exception is a fine test (`tests/gct_fine.rs`), which may draw one
//! small bitmap by hand to pin a known case -- never to measure.
//!
//! A sample is settled entirely by four numbers, which is what makes a
//! result reproducible: the same seed, side, density and cluster weight
//! give the same bitmap on every run and every machine. Nothing is
//! stored: a corpus is regenerated every time it is asked for, which
//! costs almost nothing and means there is no file to fall out of step
//! with the code that reads it.
//!
//! Hand-drawn shapes used to live here -- unions of circles, a
//! checkerboard, three tiled motifs found by search. They are gone, and
//! little was lost: the generator reaches worse ground than any of
//! them. The worst motif cost 13,057 instructions per set cell, where
//! `grown(_, 0.50, 0.00)` costs 124,610.
//!
//! Two families live here. [`SHAPES`] are grown: cells scattered or
//! clustered to a density, which is what an algorithm is stressed on.
//! [`PLANS`] are laid out: streets, blocks and courtyards on a grid
//! the quadtree can see, which is the shape the encoding is for. A
//! result measured on one and not the other has been measured on
//! half of what matters.
//!
//! `generate` holds the drawing itself, `city` holds the laying out,
//! and `docs/testing_protocol.md` holds
//! the rule for using it: fix with the seed held still, then check on a
//! seed never seen. A change measured only on the corpus it was tuned
//! on has not been measured.

mod city;
mod generate;
pub mod seed;

pub use city::{one_laid_out, Cities, Plan, PLANS};
pub use seed::seed_for_group;

use crate::Bitmap;

/// Where a sample group's seeds start, read from
/// [`seed::WHERE_THE_SEED_IS_KEPT`] rather than written here.
///
/// Nothing a measurement runs on is a constant in the code. Move the
/// seed to ask whether a result was about an algorithm or about those
/// particular bitmaps, and the run says which seed each group used.
pub fn sample_seed(group: &str) -> u64 {
    seed::seed_for_group(group)
}

/// One shape worth measuring on: what it looks like, the two numbers
/// that make it, and how many of it a timed run should take.
pub struct Shape {
    pub name: &'static str,
    /// The share of the cells that end up set.
    pub density: f64,
    /// How often a new cell lands beside one already set rather than
    /// anywhere at all.
    pub cluster: f64,
    /// How many to time over. Not the same for every shape, and it
    /// cannot be: a dense ragged bitmap runs to ten thousand rectangles
    /// and costs a thousand times what a sparse one does, so one count
    /// would be either too few to average or too slow to finish.
    pub timed: u64,
    /// How many to check in a unit test, where the budget is a second
    /// rather than a minute.
    pub tested: u64,
}

impl Shape {
    /// `count` bitmaps of this shape, built one at a time.
    pub fn take(&self, count: u64) -> Samples {
        grown(sample_seed(self.name), self.density, self.cluster, count)
    }

    /// The same, confined to a `side` by `side` corner.
    pub fn take_in(&self, side: usize, count: u64) -> Samples {
        grown_in(sample_seed(self.name), side, self.density, self.cluster, count)
    }

    /// As many as a timed run of this shape should take.
    pub fn timed(&self) -> Samples {
        self.take(self.timed)
    }

    /// As many as a unit test of this shape should take.
    pub fn tested(&self) -> Samples {
        self.take(self.tested)
    }
}

/// The settings worth measuring on, named for what they look like.
///
/// Density and cluster weight between them span the cases that matter.
/// Scattered cells at any density are almost all forced 1x1 and cost
/// nothing; solid blobs are few big rectangles; the ragged middle is
/// where both algorithms work hardest and where the gap between them
/// is widest.
pub const SHAPES: [Shape; 9] = [
    Shape { name: "sparse scattered", density: 0.05, cluster: 0.00, timed: 20, tested: 2 },
    Shape { name: "sparse ragged", density: 0.05, cluster: 0.70, timed: 20, tested: 2 },
    Shape { name: "sparse blobs", density: 0.05, cluster: 0.95, timed: 20, tested: 2 },
    Shape { name: "middling scattered", density: 0.20, cluster: 0.00, timed: 6, tested: 1 },
    Shape { name: "middling ragged", density: 0.20, cluster: 0.70, timed: 6, tested: 1 },
    Shape { name: "middling blobs", density: 0.20, cluster: 0.95, timed: 6, tested: 1 },
    Shape { name: "dense scattered", density: 0.50, cluster: 0.00, timed: 2, tested: 1 },
    Shape { name: "dense ragged", density: 0.50, cluster: 0.70, timed: 2, tested: 1 },
    Shape { name: "dense blobs", density: 0.50, cluster: 0.95, timed: 2, tested: 1 },
];

/// Which of them a general benchmark runs on: enough content to be
/// connected, ragged enough to be mostly boundary.
pub fn typical() -> &'static Shape {
    &SHAPES[4]
}

/// A run of bitmaps from consecutive seeds, built one at a time.
///
/// Lazy because a caller measuring two thousand of them has no reason
/// to hold two thousand at once, and because the sweeps that compare
/// seed ranges would otherwise spend their first seconds allocating.
pub struct Samples {
    seed: u64,
    left: u64,
    side: usize,
    density: f64,
    cluster: f64,
}

/// `count` bitmaps from seeds `seed`, `seed + 1`, and so on.
///
/// `density` is the share of the 65536 cells set in each. `cluster` is
/// how often a new cell lands beside one already set rather than
/// anywhere at all: at 0 the cells are scattered and every one is its
/// own rectangle, at 1 they only extend what is standing and the
/// bitmap is a few solid blobs. Between them the two parameters cover
/// the cases that used to be drawn by hand.
pub fn grown(seed: u64, density: f64, cluster: f64, count: u64) -> Samples {
    grown_in(seed, crate::WIDTH, density, cluster, count)
}

/// The same, confined to a `side` by `side` corner of the matrix.
///
/// For callers that cannot afford a full one: exhaustive search is
/// exponential in the cells, so the ground truth runs on corners of
/// eight or fewer.
pub fn grown_in(seed: u64, side: usize, density: f64, cluster: f64, count: u64) -> Samples {
    Samples { seed, left: count, side, density, cluster }
}

/// One bitmap, for a caller that wants a single sample rather than a
/// run of them.
pub fn one_grown(seed: u64, density: f64, cluster: f64) -> Bitmap {
    generate::one(seed, crate::WIDTH, density, cluster)
}

impl Iterator for Samples {
    type Item = Bitmap;

    fn next(&mut self) -> Option<Bitmap> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        let seed = self.seed;
        self.seed += 1;
        Some(generate::one(seed, self.side, self.density, self.cluster))
    }
}

impl ExactSizeIterator for Samples {
    fn len(&self) -> usize {
        self.left as usize
    }
}

/// Every family of sample, named, with as many of each as a
/// measurement should take.
///
/// Two families, and they disagree about what the encoding is for.
/// Grown bitmaps are cells scattered or clustered to a density, which
/// is what an algorithm is stressed on. Laid out ones are streets,
/// blocks and courtyards on a grid the quadtree can see, which is the
/// shape the encoding was designed around. A result on one is half a
/// result.
pub fn every_family() -> Vec<(String, Vec<BitmapSample>)> {
    vec![
        (
            "laid out like a city".to_string(),
            PLANS.iter().flat_map(|plan| plan.timed()).collect(),
        ),
        (
            "grown like a blob".to_string(),
            SHAPES.iter().flat_map(|shape| shape.timed()).collect(),
        ),
    ]
}

/// What a family is made of.
pub type BitmapSample = crate::Bitmap;
