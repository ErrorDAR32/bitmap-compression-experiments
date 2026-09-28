//! zstd on the raw rows: a general-purpose compressor, knowing nothing
//! of images -- a reference point, not a bitmap codec. Its frame carries
//! a few bytes of header.

use super::Codec;
use crate::rows::{Rows, BYTES};
use bitmap::Bitmap;
use zstd::bulk::{Compressor, Decompressor};

/// zstd at one level, with its contexts and buffers kept between
/// bitmaps.
pub struct Zstd {
    /// The level compressed at.
    level: i32,
    /// The compression context.
    compressor: Compressor<'static>,
    /// The decompression context.
    decompressor: Decompressor<'static>,
    /// The last encoding.
    encoded: Vec<u8>,
    /// The last rows decoded.
    decoded: Rows,
}

impl Zstd {
    /// zstd at `level`.
    pub fn new(level: i32) -> Self {
        Self {
            level,
            compressor: Compressor::new(level).expect("a zstd context"),
            decompressor: Decompressor::new().expect("a zstd context"),
            encoded: Vec::with_capacity(zstd::zstd_safe::compress_bound(BYTES)),
            decoded: Rows(vec![0; BYTES]),
        }
    }
}

impl Codec for Zstd {
    fn name(&self) -> String {
        format!("zstd level {}", self.level)
    }

    fn encode(&mut self, _: &Bitmap, rows: &Rows) {
        self.encoded.clear();
        self.compressor.compress_to_buffer(&rows.0, &mut self.encoded).expect("room for the bound");
    }

    fn encoded_bits(&self) -> usize {
        self.encoded.len() * 8
    }

    fn decode(&mut self) {
        let written = self.decompressor.decompress_to_buffer(&self.encoded, &mut self.decoded.0[..]).expect("a stream zstd wrote");
        assert_eq!(written, BYTES);
    }

    fn decoded_matches(&self, rows: &Rows) -> bool {
        self.decoded == *rows
    }
}
