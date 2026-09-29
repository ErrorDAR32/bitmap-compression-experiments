//! The codecs compared, one file each, behind one interface: encode a
//! bitmap, decode it back, check it came back whole.

pub mod g4;
pub mod tessera;
pub mod jbig;
pub mod zstd;

use crate::rows::Rows;
use bitmap::Bitmap;

/// One codec, holding its own room and its last output.
pub trait Codec {
    /// What the tables call it.
    fn name(&self) -> String;

    /// Encodes one bitmap, given both as Tessera takes it and in rows. Only
    /// this is timed as encoding.
    fn encode(&mut self, bitmap: &Bitmap, rows: &Rows);

    /// The last encoding's size, in bits.
    fn encoded_bits(&self) -> usize;

    /// Decodes what [`Codec::encode`] last wrote. Only this is timed as
    /// decoding.
    fn decode(&mut self);

    /// Whether the last decode gave back `rows`. Not timed.
    fn decoded_matches(&self, rows: &Rows) -> bool;
}
