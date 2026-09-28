//! A fixed sample for counting gct's instructions: every shape, sparse
//! shape, plan and line set at its `tested` count, a checkerboard and
//! noise -- encoded, then decoded, all in one workspace. Run it under
//! callgrind, which counts executed instructions exactly and says where
//! they go:
//!
//! ```text
//! cargo build --release --example gct_instruction_count
//! valgrind --tool=callgrind --callgrind-out-file=target/callgrind.out \
//!     target/release/examples/gct_instruction_count
//! callgrind_annotate target/callgrind.out | head -40
//! ```
//!
//! Instructions a bitmap are the total over the bitmap count it prints.

use bitmap::gct::grammar::bit_stream::BitStream;
use bitmap::gct::Workspace;
use bitmap::samples::checkerboards::checkerboard;
use bitmap::samples::{grown, LINE_SETS, PLANS, SHAPES, SPARSE};
use bitmap::Bitmap;

/// The checkerboard in the sample: odd squares, so nothing lines up.
const CHECKERBOARD_SQUARE: u8 = 7;
/// Noise in the sample: half the cells, scattered...
const NOISE_DENSITY: f64 = 0.5;
/// ...from a fixed seed...
const NOISE_SEED: u64 = 1;
/// ...one bitmap of it.
const NOISE_BITMAPS: u64 = 1;

/// Builds the sample, then encodes and decodes every bitmap of it.
fn main() {
    let mut sample: Vec<Bitmap> = Vec::new();
    for shape in SHAPES.iter().chain(&SPARSE) {
        sample.extend(shape.tested());
    }
    for plan in &PLANS {
        sample.extend(plan.tested());
    }
    for set in &LINE_SETS {
        sample.extend(set.tested());
    }
    sample.push(checkerboard(CHECKERBOARD_SQUARE));
    sample.extend(grown(NOISE_SEED, NOISE_DENSITY, 0.0, NOISE_BITMAPS));

    // One workspace, stream and bitmap for the whole sample, as a caller
    // encoding many would keep them.
    let (mut workspace, mut stream, mut back) = (Workspace::new(), BitStream::default(), Bitmap::new());
    let mut bits = 0;
    for bitmap in &sample {
        workspace.encode(bitmap, &mut stream);
        bits += stream.len();
        workspace.decode(&stream, &mut back);
        assert_eq!(back.count_set(), bitmap.count_set());
    }
    println!("{} bitmaps, {} bits", sample.len(), bits);
}
