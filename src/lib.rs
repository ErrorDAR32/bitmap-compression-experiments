//! A fixed-size 256x256 bit matrix, and an encoding of it.
//!
//! [`dsrn`] is disjoint sized-tile region nesting: the bitmap read as a
//! quadtree whose regions are bound to a tile size, each bound region
//! emitting one value per tile, and whatever no tile size describes
//! copied from a neighbour that looks the same.
//!
//! # Using it
//!
//! The workspaces hold every buffer the encoding needs, so they are
//! built once and fed bitmap after bitmap:
//!
//! ```
//! use bitmatrix::dsrn::passes::{decode, encode, Encoded, Work};
//! use bitmatrix::dsrn::rules::Ruleset;
//! use bitmatrix::dsrn::Pyramid;
//! use bitmatrix::BitMatrix;
//!
//! let mut bits = BitMatrix::new();
//! bits.set_rect(10, 10, 40, 30);
//! bits.set_circle(180, 180, 25);
//!
//! let (mut pyramid, mut work) = (Pyramid::new(), Work::default());
//! let (mut out, mut back) = (Encoded::default(), BitMatrix::new());
//!
//! pyramid.rebuild(&bits);
//! encode(&pyramid, &bits, Ruleset::ALL[0], &mut work, &mut out);
//! decode(&out, Ruleset::ALL[0], &mut work, &mut back);
//! assert_eq!(back.count_set(), bits.count_set());
//! ```
//!
//! # How the crate is laid out
//!
//! | module | what lives there |
//! |---|---|
//! | `data` | the bitmap and the bit operations on it, and nothing that decides anything |
//! | [`dsrn`] | the pyramid every decision is asked of, and the passes that ask |
//! | [`samples`] | the one source of test bitmaps |
//!
//! `docs/protocol.md` is how a change to any of it gets measured, and
//! is worth reading before trusting a number in these comments.
//!
//! # What it costs
//!
//! An encode is about 2.15M instructions a bitmap, of which the
//! pyramid is 47k: the passes are nearly all of it. What it emits
//! depends entirely on the content -- a checkerboard of single cells
//! comes to 120 bits, one of 2x2 blocks to 16,400 -- so the `dsrn_size`
//! and `dsrn_patterns` examples are worth reading before trusting an
//! average.

pub mod dsrn;
mod data;
pub mod samples;

pub use data::BitMatrix;

/// The matrix is always this wide and this tall. Nothing is sized at
/// run time, which is what lets a workspace be built once and reused.
pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 256;

pub(crate) const BITS_PER_WORD: usize = 64;
pub(crate) const WORDS: usize = (WIDTH * HEIGHT) / BITS_PER_WORD;
