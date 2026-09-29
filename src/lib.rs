//! A fixed size 256 by 256 bitmap, and an encoding of it.
//!
//! [`gct`] is the greedy complex tiler: the bitmap tiled greedily,
//! biggest first, by tiles bound to one value or copied from a
//! neighbour; those grouped into complex tiles, each said at one
//! resolution; the result read as a tree and spelled out in bits.
//!
//! # Using it
//!
//! A [`gct::Workspace`] holds everything encoding and decoding need,
//! allocated once; keep one, a stream and a bitmap, and every bitmap
//! after the first is encoded and decoded without allocating.
//!
//! ```
//! use bitmap::gct::grammar::bit_stream::BitStream;
//! use bitmap::gct::Workspace;
//! use bitmap::Bitmap;
//!
//! let mut bitmap = Bitmap::new();
//! bitmap.set_rect(10, 10, 40, 30);
//! bitmap.set_circle(180, 180, 25);
//!
//! let (mut workspace, mut stream, mut back) = (Workspace::new(), BitStream::default(), Bitmap::new());
//! workspace.encode(&bitmap, &mut stream);
//! workspace.decode(&stream, &mut back);
//! assert_eq!(back.count_set(), bitmap.count_set());
//! ```
//!
//! For a single bitmap, [`gct::encode`](fn@gct::encode) and
//! [`gct::decode`](fn@gct::decode) do the same
//! in a workspace of their own.
//!
//! # How the crate is laid out
//!
//! One folder to a domain, and inside it one file to a purpose.
//!
//! | folder | its domain |
//! |---|---|
//! | [`adversarial`] | searches for the bitmaps an encoder does worst on, by any score |
//! | [`bitmap`] | the 65536 cells, and what can be drawn on them |
//! | [`diagnostics`] | data gathered from gct's steps and output, for the tests to judge and the tools to print |
//! | [`gct`] | the greedy complex tiler, the encoding (`docs/gct.md`) |
//! | `fixed_list` | the one list gct keeps: a fixed capacity, allocated once, never growing |
//! | `morton` | the Morton order the bitmap and every pyramid level are laid out in |
//! | [`rng`] | the one seeded random source, for the samples and the searches |
//! | [`samples`] | the bitmaps everything is measured on, and where the seed comes from |
//! | [`table`] | printing any of it the same way, and keeping measurements in `measurements/` |
//!
//! `tests/` holds gct's tests, which judge what [`diagnostics`] gathers;
//! `src/bin/` the diagnostics tool, which prints it, and the adversarial
//! search against the raw cells; `examples/` the timing and instruction
//! count.
//!
//! `docs/design_statements.md` is what every decision here is weighed
//! against, and `docs/testing_protocol.md` is how a change to any of
//! it gets measured.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

pub mod adversarial;
pub mod bitmap;
pub mod diagnostics;
mod fixed_list;
mod morton;
pub mod gct;
pub mod rng;
pub mod samples;
pub mod table;

pub use bitmap::Bitmap;

/// The bitmap is always this wide... Nothing is sized at run time,
/// which is what lets a workspace be built once and reused.
pub const WIDTH: usize = 256;
/// ...and this tall.
pub const HEIGHT: usize = 256;

/// Cells a word holds.
pub(crate) const BITS_PER_WORD: usize = 64;
/// Words a bitmap takes.
pub(crate) const WORDS: usize = (WIDTH * HEIGHT) / BITS_PER_WORD;
