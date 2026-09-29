//! What every Tessera test checks of a bitmap, shared by the three tiers:
//! judgements on what `tessera::diagnostics` gathers.

use tessera::diagnostics::examination::Examination;
use tessera::diagnostics::RAW_CELLS;
use tessera::grammar::bit_stream::BitStream;
use tessera::Tessera;
use bitmap::Bitmap;
use std::cell::RefCell;

/// The most Tessera may ever spend on a bitmap: the raw cells and 1%.
pub const CAP_BITS: usize = RAW_CELLS + RAW_CELLS / 100;

/// Everything that has to hold of one bitmap, each failure naming what
/// broke:
///
/// - every cell is said by exactly one of the greedy tiler's placed
///   tiles, or is said in a residual block, or lies in a 2x2 that is not
///   homogeneous, placed nothing, and is inside a complex tile of 1x1
///   resolution or a cell list;
/// - nothing finer than a 2x2 is placed, nothing finer than 4x4 copied;
/// - the reference bit cost the greedy tiler scores with is the tree's written
///   bits, and the bits counted for the encoding that suits the bitmap
///   -- the tree, or the count split -- are the stream's;
/// - Tessera spends at most [`CAP_BITS`], the raw cells and 1%;
/// - the tree read back from its bits is the tree that was written, and
///   both the tree's bits and the stream's decode to every cell.
pub fn check(bitmap: &Bitmap, label: &str) {
    thread_local! {
        /// One `Tessera` for every check a thread makes, so every test
        /// also checks that nothing one bitmap leaves in it leaks into
        /// the next.
        static TESSERA: RefCell<(Tessera, BitStream, Bitmap)> =
            RefCell::new((Tessera::new(), BitStream::default(), Bitmap::new()));
    }
    TESSERA.with_borrow_mut(|(tessera, stream, back)| {
        let examined = Examination::of(tessera, stream, back, bitmap);
        assert_eq!(examined.coverage_faults.first(), None, "{label}: a cell said wrongly");
        assert_eq!(examined.placed_finer_than_2x2.first(), None, "{label}: placed finer than a 2x2");
        assert_eq!(examined.copied_finer_than_4x4.first(), None, "{label}: copied finer than 4x4");
        assert_eq!(examined.tree_bits, examined.tree_written_bits, "{label}: the reference bit cost is not the tree's written bits");
        assert_eq!(examined.counted_bits, examined.written_bits as u64, "{label}: the counted bits are not the encoder's count");
        assert!(examined.written_bits <= CAP_BITS, "{label}: {} bits, over the cap of {CAP_BITS}", examined.written_bits);
        assert!(examined.tree_read_back, "{label}: the tree read back is not the tree written");
        assert_eq!(examined.tree_first_difference, None, "{label}: the tree's bits do not decode to the bitmap");
        if let Some((x, y)) = examined.first_difference {
            panic!("{label}: cell ({x}, {y}) comes back {} instead of {}", back.get(x, y), bitmap.get(x, y));
        }
    });
}
