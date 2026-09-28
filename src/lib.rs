//! A fixed size 256 by 256 bitmap, and an encoding of it.
//!
//! [`gct`] is the greedy complex tiler: the bitmap tiled greedily,
//! biggest first, by tiles bound to one value or copied from a
//! neighbour; those grouped into complex tiles, each said at one
//! resolution; the result read as a tree and spelled out in bits.
//!
//! # Using it
//!
//! ```
//! use bitmap::gct::{decode, encode};
//! use bitmap::Bitmap;
//!
//! let mut bitmap = Bitmap::new();
//! bitmap.set_rect(10, 10, 40, 30);
//! bitmap.set_circle(180, 180, 25);
//!
//! let stream = encode(&bitmap);
//! let back = decode(&stream);
//! assert_eq!(back.count_set(), bitmap.count_set());
//! ```
//!
//! # How the crate is laid out
//!
//! One folder to a domain, and inside it one file to a purpose.
//!
//! | folder | its domain |
//! |---|---|
//! | [`bitmap`] | the 65536 cells, and what can be drawn on them |
//! | [`gct`] | the greedy complex tiler, the encoding (`docs/gct.md`) |
//! | `morton` | the Morton order the bitmap and every pyramid level are laid out in |
//! | [`samples`] | the bitmaps everything is measured on, and where the seed comes from |
//! | [`table`] | printing any of it, which every measurement does the same way |
//!
//! `tests/` holds gct's tests, its measurement, the adversarial search
//! and the diagnostics.
//!
//! `docs/design_statements.md` is what every decision here is weighed
//! against, and `docs/testing_protocol.md` is how a change to any of
//! it gets measured.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

pub mod bitmap;
mod morton;
pub mod gct;
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
