//! Wall-clock time to encode, averaged over a large sample: every
//! generator's bitmaps -- grown shapes, sparse ones, city plans and line
//! sets -- `BITMAPS_PER_GENERATOR` distinct bitmaps each, all built
//! before any is timed, then each encoded once in one workspace. Decoding
//! is timed apart, after. The saved adversarial patterns (`testing/adversarial/saved/`)
//! get a row of their own, apart from the sample's. Run it in release, on
//! its own -- no profiler, nothing else busy:
//!
//! ```text
//! cargo run --release --example gct_timing
//! cargo run --release --example gct_timing -- 400
//! ```
//!
//! The argument, if given, is how many bitmaps each generator makes.

use bitmap::adversarial::record;
use bitmap::gct::grammar::bit_stream::BitStream;
use bitmap::gct::Workspace;
use bitmap::samples::{LINE_SETS, PLANS, SHAPES, SPARSE};
use bitmap::Bitmap;
use std::time::{Duration, Instant};

/// Bitmaps each generator makes unless told otherwise: 20 generators,
/// so 2000 bitmaps -- enough for a steady mean and a tail.
const BITMAPS_PER_GENERATOR: u64 = 100;

/// Times each adversarial record is encoded: they are few, so each is
/// timed often enough to average.
const RECORD_REPEATS: usize = 20;

/// Percentile reported as the tail, besides the worst.
const TAIL_PERCENT: usize = 90;

/// One family's bitmaps, named.
struct Family {
    /// What the family is called.
    name: &'static str,
    /// Its bitmaps.
    bitmaps: Vec<Bitmap>,
}

/// Builds the sample, times encoding every bitmap of it once, then
/// decoding, and prints both a family at a time.
fn main() {
    let per_generator = std::env::args().nth(1).map_or(BITMAPS_PER_GENERATOR, |count| count.parse().expect("a count"));
    let families = [
        Family { name: "grown like a blob", bitmaps: SHAPES.iter().flat_map(|shape| shape.take(per_generator)).collect() },
        Family { name: "sparse", bitmaps: SPARSE.iter().flat_map(|shape| shape.take(per_generator)).collect() },
        Family { name: "laid out like a city", bitmaps: PLANS.iter().flat_map(|plan| plan.take(per_generator)).collect() },
        Family { name: "drawn with lines", bitmaps: LINE_SETS.iter().flat_map(|set| set.take(per_generator)).collect() },
    ];

    let (mut workspace, mut stream, mut back) = (Workspace::new(), BitStream::default(), Bitmap::new());
    // One encode first, so the first timed one pays for nothing extra.
    workspace.encode(&families[0].bitmaps[0], &mut stream);

    println!("{:<22} {:>8} {:>10} {:>10} {:>10} {:>10} {:>11}", "family", "bitmaps", "mean us", "median us", "p90 us", "max us", "decode us");
    let (mut every_encode, mut every_decode) = (Vec::new(), Vec::new());
    for family in &families {
        let (mut encodes, decodes) = time(&mut workspace, &mut stream, &mut back, family);
        print_row(family.name, &mut encodes, &decodes);
        every_encode.extend(encodes);
        every_decode.extend(decodes);
    }
    print_row("all", &mut every_encode, &every_decode);

    // The saved adversarial patterns, apart from the sample: few, and
    // each among the worst found against one encoder, so each is encoded
    // several times.
    let records = record::saved();
    let records = Family {
        name: "adversarial saved",
        bitmaps: (0..RECORD_REPEATS).flat_map(|_| records.iter().map(|(_, bitmap)| bitmap.clone())).collect(),
    };
    let (mut encodes, decodes) = time(&mut workspace, &mut stream, &mut back, &records);
    print_row(records.name, &mut encodes, &decodes);
}

/// Encodes every bitmap of `family` once, timing each, then decodes each
/// stream, timing each and checking it round trips: the encode times and
/// the decode times.
fn time(workspace: &mut Workspace, stream: &mut BitStream, back: &mut Bitmap, family: &Family) -> (Vec<Duration>, Vec<Duration>) {
    let mut encodes = Vec::with_capacity(family.bitmaps.len());
    let mut streams = Vec::with_capacity(family.bitmaps.len());
    for bitmap in &family.bitmaps {
        let start = Instant::now();
        workspace.encode(bitmap, stream);
        encodes.push(start.elapsed());
        streams.push(stream.clone());
    }
    let mut decodes = Vec::with_capacity(streams.len());
    for (encoded, bitmap) in streams.iter().zip(&family.bitmaps) {
        let start = Instant::now();
        workspace.decode(encoded, back);
        decodes.push(start.elapsed());
        assert!(same_cells(back, bitmap), "{} did not round trip", family.name);
    }
    (encodes, decodes)
}

/// Prints one row: how many, the encode times' mean, median, tail and
/// worst, and the decode times' mean.
fn print_row(name: &str, encodes: &mut [Duration], decodes: &[Duration]) {
    encodes.sort_unstable();
    let micros = |duration: Duration| duration.as_secs_f64() * 1e6;
    let mean = |times: &[Duration]| times.iter().map(|&time| micros(time)).sum::<f64>() / times.len() as f64;
    let at_percent = |percent: usize| micros(encodes[(encodes.len() - 1) * percent / 100]);
    println!(
        "{:<22} {:>8} {:>10.1} {:>10.1} {:>10.1} {:>10.1} {:>11.1}",
        name,
        encodes.len(),
        mean(encodes),
        at_percent(50),
        at_percent(TAIL_PERCENT),
        micros(encodes[encodes.len() - 1]),
        mean(decodes),
    );
}

/// Whether two bitmaps hold the same cells, read one at a time: outside
/// anything timed.
fn same_cells(a: &Bitmap, b: &Bitmap) -> bool {
    (0..=u8::MAX).all(|y| (0..=u8::MAX).all(|x| a.get(x, y) == b.get(x, y)))
}
