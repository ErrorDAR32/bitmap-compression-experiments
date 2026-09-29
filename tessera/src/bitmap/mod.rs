//! The bitmap: 65536 cells, and everything that can be asked of them
//! or done to them.
//!
//! | file | what is in it |
//! |---|---|
//! | `bitmap_data` | what a bitmap is, its Morton-order layout, and what can be asked of a cell or an aligned square |
//! | `bitmap_drawing` | rectangles and circles, drawn by their shape |
//!
//! Nothing here decides anything. What to describe, at what size, in
//! what order -- all of that is the encoding's, a [`crate::Tessera`]'s, which
//! holds a bitmap and reads it.

mod bitmap_data;
mod bitmap_drawing;

pub use bitmap_data::Bitmap;
