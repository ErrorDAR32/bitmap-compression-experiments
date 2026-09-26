//! The bits an encode produces.
//!
//! | file | what is in it |
//! |---|---|
//! | `stream_data` | an encoded bitmap, written in order and read by position |

mod stream_data;

pub use stream_data::EncodedBitmap;
