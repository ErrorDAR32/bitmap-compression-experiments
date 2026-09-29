//! tilesim: a fixed size 256 by 256 bitmap, and an encoding of it.
//!
//! [`tessera`] is the encoding, a greedy tiler: the bitmap tiled greedily,
//! biggest first, by tiles bound to one value or copied from a
//! neighbour; those grouped into complex tiles, each said at one
//! resolution; the result read as a tree and spelled out in bits.
//!
//! # Using it
//!
//! A [`tessera::Tessera`] holds everything encoding and decoding need,
//! allocated once; keep one, a stream and a bitmap, and every bitmap
//! after the first is encoded and decoded without allocating.
//!
//! ```
//! use tilesim::tessera::grammar::bit_stream::BitStream;
//! use tilesim::tessera::Tessera;
//! use tilesim::Bitmap;
//!
//! let mut bitmap = Bitmap::new();
//! bitmap.set_rect(10, 10, 40, 30);
//! bitmap.set_circle(180, 180, 25);
//!
//! let (mut Tessera, mut stream, mut back) = (Tessera::new(), BitStream::default(), Bitmap::new());
//! Tessera.encode(&bitmap, &mut stream);
//! Tessera.decode(&stream, &mut back);
//! assert_eq!(back.count_set(), bitmap.count_set());
//! ```
//!
//! For a single bitmap, [`tessera::encode`](fn@tessera::encode) and
//! [`tessera::decode`](fn@tessera::decode) do the same
//! with a `Tessera` of their own.
//!
//! # How the crate is laid out
//!
//! One folder to a domain, and inside it one file to a purpose.
//!
//! | folder | its domain |
//! |---|---|
//! | [`adversarial`] | searches for the bitmaps an encoder does worst on, by any score |
//! | [`bitmap`] | the 65536 cells, and what can be drawn on them |
//! | [`diagnostics`] | data gathered from Tessera's steps and output, for the tests to judge and the tools to print |
//! | [`tessera`] | Tessera, the encoding, a greedy tiler with complex tiles (`docs/tessera.md`), with the fixed-capacity list it keeps |
//! | `morton` | the Morton order the bitmap and every pyramid level are laid out in |
//! | [`rng`] | the one seeded random source, for the sample generators and the searches |
//! | [`sample_generators`] | the bitmaps everything is measured on, and where the seed comes from |
//! | [`table`] | printing any of it the same way, and keeping measurements in `docs/measurements/` |
//!
//! `tests/` holds Tessera's tests, which judge what [`diagnostics`] gathers,
//! and `tests/last_seed`, the seed every seeded run uses, kept out of git; `src/tools/` the
//! diagnostics tool, which prints it -- timing and the instruction count
//! among its tools -- and the adversarial search against the raw cells.
//! `external_benchmarks/` is a crate of its own: Tessera against existing
//! bitmap compressors, the adversarial searches against each, and the
//! bitmaps those found.
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
mod morton;
pub mod tessera;
pub mod rng;
pub mod sample_generators;
pub mod table;

pub use bitmap::Bitmap;

/// The bitmap is always this wide... Nothing is sized at run time,
/// which is what lets a `Tessera` be built once and reused.
pub const WIDTH: usize = 256;
/// ...and this tall.
pub const HEIGHT: usize = 256;

/// Cells a word holds.
pub(crate) const BITS_PER_WORD: usize = 64;
/// Words a bitmap takes.
pub(crate) const WORDS: usize = (WIDTH * HEIGHT) / BITS_PER_WORD;
