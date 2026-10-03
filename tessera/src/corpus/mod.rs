//! The one source of corpus bitmaps, tests and measurements alike, so
//! changing what anything runs on is a change here. Two exceptions: a
//! fine test (`tests/fine.rs`) may draw one small bitmap by hand to
//! pin a known case, never to measure; and [`checkerboards`] are drawn,
//! the one family the same on every seed -- squares of an odd side never
//! line up with the power-of-two grid, so one board exercises homogeneous
//! tiles, cuts, copies and copies naming children at once.
//!
//! A corpus bitmap is settled by its seed and its generator's parameters: the
//! same ones give the same bitmap on every run and every machine.
//! Nothing is stored; a corpus is grown again every time it is asked
//! for. Four families of generator:
//!
//! - grown ([`SHAPES`], `generate.rs`): cells scattered or clustered to
//!   a density;
//! - sparse ([`SPARSE`]): grown too thin to be a shape at all;
//! - laid out ([`PLANS`], `city.rs`): streets, blocks and courtyards on a
//!   grid at an offset of its own, never on the quadtree's;
//! - drawn ([`LINE_SETS`], `lines.rs`): lines, straight and diagonal,
//!   thin and wide.
//!
//! `docs/testing_protocol.md` holds the rule for using them: fix with
//! the seed held still, then check on a seed never seen.
//!
//! Function by function: `docs/lab.md`, "`corpus/`".

pub mod checkerboards;
mod city;
mod generate;
mod lines;
pub mod seed;

pub use city::{one_laid_out, Cities, Plan, PLANS};
pub use lines::{one_drawn, Drawings, LineSet, LINE_SETS};
pub use seed::seed_uncounted;

use bitmap::Bitmap;

/// Where every corpus bitmap's seeds start, read from
/// [`seed::WHERE_THE_SEED_IS_KEPT`] rather than written here, counted as
/// a use of it ([`seed::seed_counted`]).
///
/// Nothing a measurement runs on is a constant in the code. Move the
/// seed to ask whether a result was about an algorithm or about those
/// particular bitmaps, and the run says which seed it used.
pub fn corpus_seed() -> u64 {
    seed::seed_counted()
}

/// One shape worth measuring on: what it looks like, the two numbers
/// that make it, and how many of it a timed run should take.
pub struct Shape {
    /// What a measurement calls it.
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
    pub fn take(&self, count: u64) -> Grown {
        grown(corpus_seed(), self.density, self.cluster, count)
    }

    /// As many as a timed run of this shape should take.
    pub fn timed(&self) -> Grown {
        self.take(self.timed)
    }

    /// As many as a unit test of this shape should take.
    pub fn tested(&self) -> Grown {
        self.take(self.tested)
    }
}

/// The grown shapes worth measuring on, named for what they look like:
/// density and cluster weight between them span scattered cells, solid
/// blobs and the ragged middle.
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

/// Sparse bitmaps: a handful of cells to a hundredth of them, grown the
/// same way, where almost everything is clear and each set cell is what
/// the bits are spent on.
pub const SPARSE: [Shape; 11] = [
    Shape { name: "a cell or two", density: 0.00002, cluster: 0.00, timed: 12, tested: 2 },
    Shape { name: "a few bunched cells", density: 0.0001, cluster: 0.95, timed: 12, tested: 2 },
    Shape { name: "a few cells", density: 0.0002, cluster: 0.00, timed: 12, tested: 2 },
    Shape { name: "a tenth of a percent", density: 0.001, cluster: 0.00, timed: 12, tested: 2 },
    Shape { name: "a tenth of a percent, bunched", density: 0.001, cluster: 0.95, timed: 12, tested: 2 },
    Shape { name: "a hundred cells", density: 0.0015, cluster: 0.00, timed: 12, tested: 2 },
    Shape { name: "sparse clusters", density: 0.005, cluster: 0.70, timed: 12, tested: 2 },
    Shape { name: "one percent", density: 0.01, cluster: 0.00, timed: 12, tested: 2 },
    Shape { name: "one percent, bunched", density: 0.01, cluster: 0.95, timed: 12, tested: 2 },
    Shape { name: "two percent", density: 0.02, cluster: 0.00, timed: 12, tested: 2 },
    Shape { name: "two percent, bunched", density: 0.02, cluster: 0.95, timed: 12, tested: 2 },
];

/// A run of grown bitmaps from consecutive seeds, built one at a time,
/// so a caller measuring thousands never holds them all.
pub struct Grown {
    /// The next bitmap's seed.
    seed: u64,
    /// How many bitmaps are still to come.
    left: u64,
    /// The share of the cells each bitmap ends up with set.
    density: f64,
    /// How often a new cell lands beside one already set rather than
    /// anywhere at all.
    cluster: f64,
}

/// `count` bitmaps from seeds `seed`, `seed + 1`, and so on.
///
/// `density` is the share of the 65536 cells set in each. `cluster` is
/// how often a new cell lands beside one already set rather than
/// anywhere at all: at 0 the cells are scattered and every one is its
/// own rectangle, at 1 they only extend what is standing and the
/// bitmap is a few solid blobs.
pub fn grown(seed: u64, density: f64, cluster: f64, count: u64) -> Grown {
    Grown { seed, left: count, density, cluster }
}

/// One bitmap, for a caller that wants a single bitmap rather than a
/// run of them.
pub fn one_grown(seed: u64, density: f64, cluster: f64) -> Bitmap {
    generate::one(seed, density, cluster)
}

impl Iterator for Grown {
    type Item = Bitmap;

    fn next(&mut self) -> Option<Bitmap> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        let seed = self.seed;
        self.seed += 1;
        Some(generate::one(seed, self.density, self.cluster))
    }
}

impl ExactSizeIterator for Grown {
    fn len(&self) -> usize {
        self.left as usize
    }
}

/// How many bitmaps a family takes from each of its generators: a
/// shape, a plan or a line set.
#[derive(Clone, Copy, Debug)]
pub enum HowMany {
    /// As many as a timed run of that generator should take: its own
    /// count.
    Timed,
    /// As many as a test of that generator should take: the fast
    /// tier's own, few.
    Tested,
    /// The same number of each.
    Each(u64),
}

impl HowMany {
    /// How many to take of a generator whose own counts are `timed` and
    /// `tested`.
    pub fn of(self, timed: u64, tested: u64) -> u64 {
        match self {
            HowMany::Timed => timed,
            HowMany::Tested => tested,
            HowMany::Each(count) => count,
        }
    }
}

/// Bitmaps each generator makes for a timing, unless told otherwise:
/// enough, over every generator, for a steady mean and a tail.
pub const TIMING_PER_GENERATOR: u64 = 100;

/// Every family of corpus bitmaps, named, with `how_many` of each generator's.
///
/// Four families. Grown bitmaps are cells scattered or clustered to a
/// density, which is what an algorithm is stressed on. Laid out ones
/// are streets, blocks and courtyards on a grid at its own offset,
/// which is structure the quadtree has to find rather than is handed.
/// Drawn ones are lines, straight and diagonal, thin and wide. Sparse
/// ones are grown too thin to be a shape at all. A result on one is a
/// quarter of a result.
pub fn families(how_many: HowMany) -> Vec<(String, Vec<Bitmap>)> {
    vec![
        ("laid out like a city".to_string(), PLANS.iter().flat_map(|plan| plan.take(how_many.of(plan.timed, plan.tested))).collect()),
        ("grown like a blob".to_string(), SHAPES.iter().flat_map(|shape| shape.take(how_many.of(shape.timed, shape.tested))).collect()),
        ("sparse".to_string(), SPARSE.iter().flat_map(|shape| shape.take(how_many.of(shape.timed, shape.tested))).collect()),
        ("drawn with lines".to_string(), LINE_SETS.iter().flat_map(|set| set.take(how_many.of(set.timed, set.tested))).collect()),
    ]
}
