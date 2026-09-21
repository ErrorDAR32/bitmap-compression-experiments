//! A fixed-size 256x256 bit matrix, and three ways to split its set
//! bits into disjoint rectangles.
//!
//! - [`RunmaxClipnmerge`] is the fast one. It reduces the bitmap to the
//!   cells standing in both orientations, meshes it into deliberately
//!   thin rectangles, and then grows them back over each other. Over
//!   540 generated bitmaps it lands 3.24% over the minimum, and costs
//!   less than [`accurate`] does on seven of the nine shapes the corpus
//!   is made of.
//! - [`accurate`] is the minimum, by the construction of Lipski and of
//!   Ohtsuki. It is what runmax-clipnmerge is measured against.
//! - Exhaustive search, the ground truth, lives in the `ground_truth`
//!   example. It is what the exact algorithm is measured against, on
//!   grids small enough to survive it.
//!
//! # Using it
//!
//! The workspace holds every buffer the algorithm needs, so it is built
//! once and fed bitmap after bitmap:
//!
//! ```
//! use bitmatrix::{BitMatrix, RunmaxClipnmerge};
//!
//! let mut bits = BitMatrix::new();
//! bits.set_rect(10, 10, 40, 30);
//! bits.set_circle(180, 180, 25);
//!
//! let mut work = RunmaxClipnmerge::new();
//! let areas = work.partition(&bits);
//! assert!(areas.iter().map(|r| r.cells()).sum::<u32>() == bits.count_set());
//! ```
//!
//! # How the crate is laid out
//!
//! The public surface is this file and the four it re-exports from.
//! Everything else is a helper, private to the crate, and each module
//! holds one idea:
//!
//! | module | what lives there |
//! |---|---|
//! | `data` | the structures both algorithms work on, and nothing that works on them |
//! | `partition` | what an algorithm is, from outside |
//! | [`runmax`] | the fast algorithm: the mesh and the moves that rewrite it |
//! | [`accurate`] | the minimum partition, which is the benchmark |
//! | [`samples`] | the one source of test bitmaps |
//!
//! The split that matters is `data` against the rest. Every structure
//! in `data` is a shape plus the questions that can be asked of it and
//! the changes that can be made to it; none of them decides anything.
//! What to take next, what to grow into, when to stop -- all of that
//! is in `runmax` and `accurate`, which hold that data and drive it.
//! [`Partition`] is what the two look like from outside, so a caller
//! can hold either without knowing which.
//!
//! `docs/walkthrough.md` works the hardest parts through by hand, one
//! line at a time, on bitmaps small enough to print.
//! `docs/protocol.md` is how a change to any of it gets measured, and
//! is worth reading before trusting a number in these comments.
//!
//! # Throughput
//!
//! A workspace holds no shared state, so one per worker thread is all
//! that parallelism needs; the test suite asserts it is [`Send`]. On
//! one core a middling ragged bitmap takes around 2.3ms, so a billion
//! bits -- 15,259 bitmaps of them -- is about half a minute, and a
//! million is about 35ms. Spread that over cores and a billion bits is
//! seconds.
//!
//! Content shapes the cost more than size does, though less wildly than
//! the hand-drawn corpus once suggested. Per set cell the spread across
//! the nine shapes is about sixfold, from 429 instructions on sparse
//! scattered content to 2604 on dense scattered: see the `cost`
//! example, which counts them under callgrind.

pub mod accurate;
mod data;
pub mod halving;
mod partition;
pub mod runmax;
pub mod samples;

pub use data::{BitMatrix, Area};
pub use partition::{assert_partition, Partition};
pub use samples::Shape;
pub use halving::Halving;
pub use runmax::{
    clip_candidates, clip_cuts, clip_with, merge_areas, merge_areas_from, mesh_by_scanning,
    Adjacency, ClipScratch, RunmaxClipnmerge, Stop,
};

/// The matrix is always this wide and this tall. Nothing is sized at
/// run time, which is what lets a workspace be built once and reused.
pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 256;

pub(crate) const BITS_PER_WORD: usize = 64;
pub(crate) const WORDS: usize = (WIDTH * HEIGHT) / BITS_PER_WORD;
