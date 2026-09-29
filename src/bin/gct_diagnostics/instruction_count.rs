//! A fixed sample for counting gct's instructions:
//! `BITMAPS_PER_GENERATOR` bitmaps of every generator -- grown shapes,
//! sparse ones, city plans and line sets, weighted as the `timing` tool's
//! sample is -- a checkerboard, every saved adversarial bitmap
//! (`external_benchmarks/adversarial/saved/`) and noise, encoded, then
//! decoded, all in one workspace. Run it under
//! callgrind, which counts executed instructions exactly and says where
//! they go:
//!
//! ```text
//! cargo build --release --bin gct_diagnostics
//! valgrind --tool=callgrind --callgrind-out-file=target/callgrind.out \
//!     target/release/gct_diagnostics instruction_count
//! callgrind_annotate target/callgrind.out | head -40
//! ```
//!
//! Instructions a bitmap are the total over the bitmap count it prints.

use tilesim::gct::grammar::bit_stream::BitStream;
use tilesim::adversarial::record;
use tilesim::gct::Workspace;
use tilesim::sample_generators::checkerboards::checkerboard;
use tilesim::diagnostics::examination::first_difference;
use tilesim::sample_generators::{families, grown, HowMany};
use tilesim::table::Table;
use tilesim::Bitmap;

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
pub fn run() {
    let mut sample: Vec<Bitmap> = families(HowMany::Each(BITMAPS_PER_GENERATOR)).into_iter().flat_map(|(_, bitmaps)| bitmaps).collect();
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
        assert_eq!(first_difference(bitmap, &back), None, "a bitmap did not round trip");
    }
    let mut table = Table::new(&["bitmaps", "bits"]);
    table.row(&[sample.len().to_string(), bits.to_string()]);
    table.print();
}
