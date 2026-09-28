//! A fixed size 256 by 256 bitmap, and an encoding of it.
//!
//! [`dsrn`] is disjoint sized-tile region nesting: the bitmap read as a
//! quadtree whose regions are each bound to a tile size, a bound
//! region emitting one value per tile, and a region no tile size suits
//! copied from a neighbour that looks the same or cut into four that
//! are easier.
//!
//! # Using it
//!
//! The workspace holds every buffer the encoding needs, so it is built
//! once and fed bitmap after bitmap:
//!
//! ```
//! use bitmap::dsrn::{decode, encode, Encoded, Knobs, Workspace};
//! use bitmap::pyramid::Pyramid;
//! use bitmap::Bitmap;
//!
//! let mut bitmap = Bitmap::new();
//! bitmap.set_rect(10, 10, 40, 30);
//! bitmap.set_circle(180, 180, 25);
//!
//! let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
//! let (mut out, mut back) = (Encoded::default(), Bitmap::new());
//!
//! pyramid.rebuild(&bitmap);
//! encode(&pyramid, &bitmap, Knobs::default(), &mut work, &mut out);
//! decode(&out, Knobs::default(), &mut back);
//! assert_eq!(back.count_set(), bitmap.count_set());
//! ```
//!
//! # How the crate is laid out
//!
//! One folder to a domain, and inside it one file to a purpose. A
//! `_data` file says what something is and what can be asked of it; a
//! file named for the folder does the work; a `_diag` file explains
//! what the work did, for a reader rather than a decoder.
//!
//! | folder | its domain |
//! |---|---|
//! | [`bitmap`] | the 65536 cells, and what can be drawn on them |
//! | [`pyramid`] | dsrn's homogeneity pyramid: for every tile of every size, whether it is all one thing |
//! | [`dsrn`] | the baseline encoding: what a region says, what it costs, how it is written and read |
//! | [`dsrn_analysis`] | experiments on dsrn, which are not the encoding |
//! | [`cgt`] | the complex greedy tiler, the encoding being built to beat dsrn (`docs/cgt.md`); depends on nothing in `dsrn` |
//! | [`samples`] | the bitmaps everything is measured on, and where the seed comes from |
//! | [`table`] | printing any of it, which every experiment does the same way |
//!
//! `tests/` holds cgt's tests and its comparison against dsrn.
//!
//! `docs/design_statements.md` is what every decision here is weighed
//! against, and `docs/testing_protocol.md` is how a change to any of
//! it gets measured.

pub mod bitmap;
pub mod cgt;
pub mod table;
pub mod dsrn;
pub mod dsrn_analysis;
pub mod pyramid;
pub mod samples;

pub use bitmap::Bitmap;

/// The bitmap is always this wide and this tall. Nothing is sized at
/// run time, which is what lets a workspace be built once and reused.
pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 256;

pub(crate) const BITS_PER_WORD: usize = 64;
pub(crate) const WORDS: usize = (WIDTH * HEIGHT) / BITS_PER_WORD;
