//! CCITT Group 4 (ITU-T T.6), the fax standard TIFF and PDF use for
//! bitmaps: each row's colour changes coded against the row above's. By
//! the pure-Rust `fax` crate (pdf-rs). Set cells are black.

use super::Codec;
use crate::rows::{Rows, BYTES, HEIGHT, ROW_BYTES, WIDTH};
use bitmap::Bitmap;
use fax::decoder::decode_g4;
use fax::encoder::Encoder;
use fax::{Color, VecWriter};

/// Group 4, with its last output and the rows last decoded.
pub struct G4 {
    /// The last encoding.
    encoded: Vec<u8>,
    /// The last rows decoded.
    decoded: Rows,
}

impl G4 {
    /// Nothing encoded yet.
    pub fn new() -> Self {
        Self { encoded: Vec::new(), decoded: Rows(vec![0; BYTES]) }
    }
}

impl Default for G4 {
    /// The same as [`G4::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl Codec for G4 {
    fn name(&self) -> String {
        "CCITT G4 (fax crate)".to_string()
    }

    fn encode(&mut self, _: &Bitmap, rows: &Rows) {
        let mut encoder = Encoder::new(VecWriter::with_capacity(BYTES));
        for row in rows.0.chunks_exact(ROW_BYTES) {
            let pels = (0..WIDTH).map(|x| if row[x / 8] & (0x80 >> (x % 8)) != 0 { Color::Black } else { Color::White });
            encoder.encode_line(pels, WIDTH as u32).expect("writing to memory");
        }
        self.encoded = encoder.finish().expect("writing to memory").finish();
    }

    fn encoded_bits(&self) -> usize {
        self.encoded.len() * 8
    }

    fn decode(&mut self) {
        let rows = &mut self.decoded.0;
        rows.fill(0);
        let mut y = 0;
        // Each row comes as the places its colour changes, white first:
        // every other run is black.
        decode_g4(self.encoded.iter().copied(), WIDTH as u32, Some(HEIGHT as u32), |changes| {
            let row = &mut rows[y * ROW_BYTES..(y + 1) * ROW_BYTES];
            for run in changes.chunks(2) {
                let (start, end) = (run[0] as usize, run.get(1).map_or(WIDTH, |&end| end as usize));
                for x in start..end.min(WIDTH) {
                    row[x / 8] |= 0x80 >> (x % 8);
                }
            }
            y += 1;
        })
        .expect("a stream G4 wrote");
    }

    fn decoded_matches(&self, rows: &Rows) -> bool {
        self.decoded == *rows
    }
}
