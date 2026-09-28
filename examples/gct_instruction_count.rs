//! A fixed sample for counting gct's instructions: `BITMAPS_PER_GENERATOR`
//! bitmaps of every generator -- grown shapes, sparse ones, city plans and
//! line sets, weighted as the timed sample (`gct_timing`) is -- a
//! checkerboard, every saved adversarial bitmap (`testing/adversarial/saved/`) and
//! noise, encoded, then decoded, all in one workspace. Run it under
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
use bitmap::adversarial::record;
use bitmap::gct::Workspace;
use bitmap::samples::checkerboards::checkerboard;
use bitmap::samples::{grown, LINE_SETS, PLANS, SHAPES, SPARSE};
use bitmap::Bitmap;

/// Bitmaps each generator makes: the first of those the timed sample
/// takes, so the counts weigh each family as the times do.
const BITMAPS_PER_GENERATOR: u64 = 5;

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
        sample.extend(shape.take(BITMAPS_PER_GENERATOR));
    }
    for plan in &PLANS {
        sample.extend(plan.take(BITMAPS_PER_GENERATOR));
    }
    for set in &LINE_SETS {
        sample.extend(set.take(BITMAPS_PER_GENERATOR));
    }
    sample.push(checkerboard(CHECKERBOARD_SQUARE));
    sample.extend(record::saved().into_iter().map(|(_, bitmap)| bitmap));
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
