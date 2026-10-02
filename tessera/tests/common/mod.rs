//! What every tier checks of a bitmap: judgements on what
//! `tessera::diagnostics` gathers.

use bitmap::Bitmap;
use std::cell::RefCell;
use tessera::diagnostics::examination::Examination;
use tessera::diagnostics::RAW_CELLS;
use tessera::{BitStream, Tessera};

/// The most Tessera may ever spend on a bitmap: the raw cells and 1%.
pub const CAP_BITS: usize = RAW_CELLS + RAW_CELLS / 100;

/// Everything that has to hold of one bitmap: it decodes to its own
/// cells, in at most [`CAP_BITS`]. Tests build in debug, so the encoder
/// also checks the tree it writes is the tree it counted. One `Tessera`
/// a thread, so nothing one bitmap leaves in it may leak into the next.
pub fn check(bitmap: &Bitmap, label: &str) {
    thread_local! {
        /// The thread's `Tessera`, stream and decoded bitmap.
        static TESSERA: RefCell<(Tessera, BitStream, Bitmap)> = RefCell::new((Tessera::new(), BitStream::default(), Bitmap::new()));
    }
    TESSERA.with_borrow_mut(|(tessera, stream, back)| {
        let examined = Examination::of(tessera, stream, back, bitmap);
        assert!(examined.written_bits <= CAP_BITS, "{label}: {} bits, over the cap of {CAP_BITS}", examined.written_bits);
        if let Some((x, y)) = examined.first_difference {
            panic!("{label}: cell ({x}, {y}) comes back {} instead of {}", back.get(x, y), bitmap.get(x, y));
        }
    });
}
