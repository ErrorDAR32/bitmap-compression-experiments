//! What every gct test checks of a bitmap, shared by the three tiers:
//! judgements on what `bitmap::diagnostics` gathers.

use bitmap::diagnostics::above_complex_tiles::AboveComplexTiles;
use bitmap::diagnostics::examination::Examination;
use bitmap::diagnostics::RAW_CELLS;
use bitmap::gct::grammar::bit_stream::BitStream;
use bitmap::gct::Workspace;
use bitmap::Bitmap;
use std::cell::RefCell;

/// The most gct may ever spend on a bitmap: the raw cells and 1%.
pub const CAP_BITS: usize = RAW_CELLS + RAW_CELLS / 100;

/// Everything that has to hold of one bitmap, each failure naming what
/// broke:
///
/// - every cell is said by exactly one of the greedy tiler's placed
///   tiles, or lies in a 2x2 that is not homogeneous, placed nothing, and
///   is said raw: a residual 2x2, or inside a complex tile of 1x1
///   resolution or a point list;
/// - nothing finer than a 2x2 is placed, nothing finer than 4x4 copied;
/// - the bit cost the complex tiler scores with is the encoder's count;
/// - the divides above the top tiles spend what the grammar says: the
///   bits written less the top tiles' own and the header;
/// - a plain Morton list of the top and background tiles would say
///   their sizes in the tree's subdivide and leaf bits, and one bit
///   more for each background tile;
/// - gct spends at most [`CAP_BITS`], the raw cells and 1%;
/// - the tree read back from the bits is the tree that was written;
/// - decoding gives back every cell.
pub fn check(bitmap: &Bitmap, label: &str) {
    thread_local! {
        /// One workspace for every check a thread makes, so every test
        /// also checks that nothing one bitmap leaves in it leaks into
        /// the next.
        static WORKSPACE: RefCell<(Workspace, BitStream, Bitmap)> =
            RefCell::new((Workspace::new(), BitStream::default(), Bitmap::new()));
    }
    WORKSPACE.with_borrow_mut(|(workspace, stream, back)| {
        let examined = Examination::of(workspace, stream, back, bitmap);
        assert_eq!(examined.coverage_faults.first(), None, "{label}: a cell said wrongly");
        assert_eq!(examined.placed_finer_than_2x2.first(), None, "{label}: placed finer than a 2x2");
        assert_eq!(examined.copied_finer_than_4x4.first(), None, "{label}: copied finer than 4x4");
        assert_eq!(examined.counted_bits, examined.written_bits as u64, "{label}: the complex tiler's bit cost is not the encoder's count");
        assert!(examined.written_bits <= CAP_BITS, "{label}: {} bits, over the cap of {CAP_BITS}", examined.written_bits);
        assert!(examined.tree_read_back, "{label}: the tree read back is not the tree written");
        if let Some((x, y)) = examined.first_difference {
            panic!("{label}: cell ({x}, {y}) comes back {} instead of {}", back.get(x, y), bitmap.get(x, y));
        }
        let above = AboveComplexTiles::of(workspace, bitmap, examined.written_bits);
        assert_eq!(above.divide_bits(), above.rest_bits, "{label}: the divides above the top tiles spend other than the grammar says");
        assert_eq!(
            above.list_size_bits(),
            above.subdivide_bits + above.leaf_bits + above.background(),
            "{label}: a plain Morton list's size bits are not the tree's subdivide and leaf bits and a bit a background tile"
        );
    });
}
