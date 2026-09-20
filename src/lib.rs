//! A fixed-size 256x256 bit matrix, and three ways to split its set
//! bits into disjoint rectangles.
//!
//! - [`RunmaxClipnmerge`] is the fast one. It reduces the bitmap to the
//!   cells standing in both orientations, meshes it into deliberately
//!   thin rectangles, and then grows them back over each other. It
//!   lands a little over the minimum, in a fifth of the time.
//! - [`exact`] is the minimum, by the construction of Lipski and of
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
//! let rects = work.partition(&bits);
//! assert!(rects.iter().map(|r| r.area()).sum::<u32>() == bits.count_set());
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
//!
//! # Throughput
//!
//! A workspace holds no shared state, so one per worker thread is all
//! that parallelism needs; the test suite asserts it is [`Send`]. On
//! one core a realistic bitmap takes around 160us, so a billion bits --
//! 15,259 bitmaps of them -- is a few seconds, and a million is a few
//! milliseconds. Content shapes that far more than size does: see the
//! `cost` example, where a ragged bitmap costs 260 times what a
//! realistic one costs per set cell.

pub mod accurate;
mod data;
mod partition;
pub mod runmax;
pub mod samples;

pub use data::{BitMatrix, Rect};
pub use partition::{assert_partition, Partition};
pub use runmax::{mesh_by_scanning, Far, RunmaxClipnmerge};

/// The matrix is always this wide and this tall. Nothing is sized at
/// run time, which is what lets a workspace be built once and reused.
pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 256;

pub(crate) const BITS_PER_WORD: usize = 64;
pub(crate) const WORDS: usize = (WIDTH * HEIGHT) / BITS_PER_WORD;
