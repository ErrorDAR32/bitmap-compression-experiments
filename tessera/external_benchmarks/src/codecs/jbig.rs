//! JBIG (ITU-T T.82), the lossless standard for bitmaps: every pixel
//! coded by an adaptive arithmetic coder on the context of the pixels
//! around it. By jbigkit, the reference C implementation, through
//! `csrc/jbig_shim.c`: one stripe, jbigkit's default options. Its
//! stream carries a 20-byte header.

use super::Codec;
use crate::rows::{Rows, BYTES, HEIGHT, WIDTH};
use tessera::Bitmap;

extern "C" {
    /// See `csrc/jbig_shim.c`.
    fn external_benchmarks_jbig_encode(rows: *mut u8, width: u64, height: u64, out: *mut u8, capacity: usize) -> usize;
    /// See `csrc/jbig_shim.c`.
    fn external_benchmarks_jbig_decode(input: *mut u8, length: usize, rows: *mut u8, rows_length: usize) -> i32;
}

/// Room for any encoding: a bitmap's own bytes twice over, with room
/// for the header -- an arithmetic coder never comes near that.
const MOST_ENCODED: usize = 2 * BYTES + 1024;

/// JBIG, with its room: the rows handed to jbigkit, its output, and the
/// rows last decoded.
pub struct Jbig {
    /// A copy of the rows being encoded: jbigkit's interface takes them
    /// mutable.
    input: Vec<u8>,
    /// Room for the output.
    encoded: Vec<u8>,
    /// Bytes of it the last encoding wrote.
    length: usize,
    /// The last rows decoded.
    decoded: Rows,
}

impl Jbig {
    /// Everything allocated once.
    pub fn new() -> Self {
        Self { input: vec![0; BYTES], encoded: vec![0; MOST_ENCODED], length: 0, decoded: Rows(vec![0; BYTES]) }
    }
}

impl Default for Jbig {
    /// The same as [`Jbig::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl Codec for Jbig {
    fn name(&self) -> String {
        "JBIG (jbigkit)".to_string()
    }

    fn encode(&mut self, _: &Bitmap, rows: &Rows) {
        self.input.copy_from_slice(&rows.0);
        // Safety: every pointer is to a buffer of the length passed with
        // it, owned here for the call.
        self.length = unsafe {
            external_benchmarks_jbig_encode(self.input.as_mut_ptr(), WIDTH as u64, HEIGHT as u64, self.encoded.as_mut_ptr(), MOST_ENCODED)
        };
        assert!(self.length > 0, "a JBIG stream larger than {MOST_ENCODED} bytes");
    }

    fn encoded_bits(&self) -> usize {
        self.length * 8
    }

    fn decode(&mut self) {
        // Safety: as for encoding.
        let result = unsafe {
            external_benchmarks_jbig_decode(self.encoded.as_mut_ptr(), self.length, self.decoded.0.as_mut_ptr(), BYTES)
        };
        assert_eq!(result, 0, "jbigkit could not decode its own stream");
    }

    fn decoded_matches(&self, rows: &Rows) -> bool {
        self.decoded == *rows
    }
}
