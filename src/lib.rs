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
//! | `matrix` | the bitmap: packed words, drawing, the isolated-cell split |
//! | `rect` | the one shape everything deals in |
//! | `workspace` | the public entry point, and the mesh loop it drives |
//! | [`exact`] | the minimum partition, which is the benchmark |
//! | `bits` | word operations on a 256-bit line |
//! | `mesh` | runmax: runs, the queue, the level, the step |
//! | `grow` | the move that reclaims rectangles |
//! | `dissolve` | the moves that finish what growing started |
//! | `pass` | the buffers those two share, and the order they run in |
//!
//! `docs/walkthrough.md` works the hardest of those through by hand,
//! one line at a time, on bitmaps small enough to print.

mod bits;
mod dissolve;
pub mod exact;
mod grow;
mod matrix;
mod mesh;
mod pass;
mod rect;
mod workspace;

pub use matrix::BitMatrix;
pub use mesh::mesh_by_scanning;
pub use pass::Far;
pub use rect::Rect;
pub use workspace::RunmaxClipnmerge;

/// The matrix is always this wide and this tall. Nothing is sized at
/// run time, which is what lets a workspace be built once and reused.
pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 256;

pub(crate) const BITS_PER_WORD: usize = 64;
pub(crate) const WORDS: usize = (WIDTH * HEIGHT) / BITS_PER_WORD;
