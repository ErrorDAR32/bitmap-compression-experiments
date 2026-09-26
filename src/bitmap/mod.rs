//! The bitmap: 65536 cells, and everything that can be asked of them
//! or done to them.
//!
//! | file | what is in it |
//! |---|---|
//! | `bitmap_data` | what a bitmap is, and what can be asked of one cell at a time |
//! | `bitmap_words` | the word layout every reader of a row relies on |
//! | `bitmap_drawing` | rectangles and circles, drawn by their shape |
//!
//! Nothing here decides anything. What to describe, at what size, in
//! what order -- all of that is [`crate::dsrn`], which holds a bitmap
//! and reads it.

pub(crate) mod bitmap_words;
mod bitmap_data;
mod bitmap_drawing;

pub use bitmap_data::Bitmap;
