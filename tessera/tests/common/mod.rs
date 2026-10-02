//! What every tier checks of a bitmap: judgements on what
//! `tessera::diagnostics` gathers.

use bitmap::Bitmap;
use std::cell::RefCell;
use tessera::diagnostics::examination::Examination;
use tessera::diagnostics::RAW_CELLS;
use tessera::{BitStream, Tessera};

/// The most Tessera may ever spend on a bitmap: the raw cells and 1%.
pub const CAP_BITS: usize = RAW_CELLS + RAW_CELLS / 100;

/// Bytes put after a stream, packed to the byte, to check it decodes the
/// same: a stream ends itself, so streams can lie one after another.
const BYTES_AFTER: [[u8; 8]; 2] = [[0xFF; 8], [0x5A, 0xC3, 0x0F, 0x96, 0x3C, 0xA5, 0xF0, 0x69]];

/// Everything that has to hold of one bitmap: it decodes to its own
/// cells, in at most [`CAP_BITS`], whatever bytes follow its stream.
/// Tests build in debug, so the encoder
/// also checks the tree it writes is the tree it counted. One `Tessera`
/// a thread, so nothing one bitmap leaves in it may leak into the next.
pub fn check(bitmap: &Bitmap, label: &str) {
    thread_local! {
        /// The thread's `Tessera`, stream and decoded bitmap.
        static TESSERA: RefCell<(Tessera, BitStream, Bitmap, BitStream)> =
            RefCell::new((Tessera::new(), BitStream::default(), Bitmap::new(), BitStream::default()));
    }
    TESSERA.with_borrow_mut(|(tessera, stream, back, followed)| {
        let examined = Examination::of(tessera, stream, back, bitmap);
        assert!(examined.written_bits <= CAP_BITS, "{label}: {} bits, over the cap of {CAP_BITS}", examined.written_bits);
        if let Some((x, y)) = examined.first_difference {
            panic!("{label}: cell ({x}, {y}) comes back {} instead of {}", back.get(x, y), bitmap.get(x, y));
        }
        for after in BYTES_AFTER {
            followed.load_bytes(&[&stream.to_bytes()[..], &after].concat());
            tessera.decode(followed, back);
            assert!(back.words() == bitmap.words(), "{label}: decodes wrong with {after:02X?} after its stream");
        }
    });
}
