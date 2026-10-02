//! TileSim's bitmap: 256 by 256 cells, and everything that can be asked
//! of them or done to them. Every layer of a chunk is one; Tessera
//! (`../tessera/`) encodes them.
//!
//! | file | what is in it |
//! |---|---|
//! | `bitmap_data` | what a bitmap is, its Morton-order layout, and what can be asked of a cell or an aligned square |
//! | `bitmap_drawing` | rectangles and circles, drawn by their shape |
//! | [`morton`] | the Morton order the cells are laid out in, which any structure over the same cells can share |
//!
//! Nothing here decides anything. What to describe, at what size, in
//! what order, is for whatever reads the bitmap.

//! The design: `docs/bitmap.md`; function by function:
//! `docs/reference.md`.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

mod bitmap_data;
mod bitmap_drawing;
pub mod morton;

pub use bitmap_data::{Bitmap, CellWords};

/// The bitmap is always this wide... Nothing is sized at run time,
/// which is what lets whatever reads it size everything once.
pub const WIDTH: usize = 256;
/// ...and this tall.
pub const HEIGHT: usize = 256;

/// Cells a word holds.
pub const BITS_PER_WORD: usize = 64;
/// Words a bitmap takes.
pub const WORDS: usize = (WIDTH * HEIGHT) / BITS_PER_WORD;
