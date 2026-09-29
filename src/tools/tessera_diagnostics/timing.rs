//! Wall-clock time to encode, averaged over a large sample: every
//! generator's bitmaps -- grown shapes, sparse ones, city plans and line
//! sets -- `sample_generators::TIMING_PER_GENERATOR` distinct bitmaps each, all
//! built before any is timed, then each encoded once in one Tessera.
//! Decoding is timed apart, after. The saved adversarial bitmaps
//! (`external_benchmarks/adversarial/saved/`) get a row of their own,
//! apart from the sample's. Run it in release, on its own -- no
//! profiler, nothing else busy:
//!
//! ```text
//! cargo run --release --bin tessera_diagnostics -- timing
//! cargo run --release --bin tessera_diagnostics -- timing 400
//! ```
//!
//! The argument after the tool's name, if given, is how many bitmaps
//! each generator makes.

use tilesim::adversarial::record;
use tilesim::diagnostics::examination::first_difference;
use tilesim::tessera::grammar::bit_stream::BitStream;
use tilesim::tessera::Tessera;
use tilesim::sample_generators::{families, HowMany, TIMING_PER_GENERATOR};
use tilesim::table::report::Report;
use tilesim::table::Table;
use tilesim::Bitmap;
use std::time::{Duration, Instant};

/// Times each saved adversarial bitmap is encoded: they are few, so each
/// is timed often enough to average.
const RECORD_REPEATS: usize = 20;

/// Percentile reported as the tail, besides the worst.
const TAIL_PERCENT: usize = 90;

/// The median's percentile.
const MEDIAN_PERCENT: usize = 50;

/// Builds the sample, times encoding every bitmap of it once, then
/// decoding, and reports both a family at a time.
pub fn run(report: &mut Report) {
    let per_generator = std::env::args().nth(2).map_or(TIMING_PER_GENERATOR, |count| count.parse().expect("a count"));
    let families = families(HowMany::Each(per_generator));

    let (mut tessera, mut stream, mut back) = (Tessera::new(), BitStream::default(), Bitmap::new());
    // One encode first, so the first timed one pays for nothing extra.
    tessera.encode(&families[0].1[0], &mut stream);

    let tail = format!("p{TAIL_PERCENT} us");
    let mut table = Table::new(&["family", "bitmaps", "mean us", "median us", &tail, "max us", "decode us"]);
    let (mut every_encode, mut every_decode) = (Vec::new(), Vec::new());
    for (name, bitmaps) in &families {
        let (mut encodes, decodes) = time(&mut tessera, &mut stream, &mut back, name, bitmaps);
        table.row(&row(name, &mut encodes, &decodes));
        every_encode.extend(encodes);
        every_decode.extend(decodes);
    }
    table.rule();
    table.row(&row("all", &mut every_encode, &every_decode));

    // The saved adversarial bitmaps, apart from the sample: few, and
    // each among the worst found against one encoder, so each is encoded
    // several times.
    let saved = record::saved();
    let repeated: Vec<Bitmap> = (0..RECORD_REPEATS).flat_map(|_| saved.iter().map(|(_, bitmap)| bitmap.clone())).collect();
    let name = "adversarial, saved";
    let (mut encodes, decodes) = time(&mut tessera, &mut stream, &mut back, name, &repeated);
    table.rule();
    table.row(&row(name, &mut encodes, &decodes));
    report.note(format!("{per_generator} bitmaps a generator, the saved adversarial bitmaps {RECORD_REPEATS} times each"));
    report.add("encode and decode times", table);
}

/// Encodes every bitmap of `bitmaps`, the family `name`, once, timing
/// each, then decodes each stream, timing each and checking it round
/// trips: the encode times and the decode times.
fn time(tessera: &mut Tessera, stream: &mut BitStream, back: &mut Bitmap, name: &str, bitmaps: &[Bitmap]) -> (Vec<Duration>, Vec<Duration>) {
    let mut encodes = Vec::with_capacity(bitmaps.len());
    let mut streams = Vec::with_capacity(bitmaps.len());
    for bitmap in bitmaps {
        let start = Instant::now();
        tessera.encode(bitmap, stream);
        encodes.push(start.elapsed());
        streams.push(stream.clone());
    }
    let mut decodes = Vec::with_capacity(streams.len());
    for (encoded, bitmap) in streams.iter().zip(bitmaps) {
        let start = Instant::now();
        tessera.decode(encoded, back);
        decodes.push(start.elapsed());
        assert_eq!(first_difference(back, bitmap), None, "{name} did not round trip");
    }
    (encodes, decodes)
}

/// One row: how many, the encode times' mean, median, tail and worst,
/// and the decode times' mean, in microseconds.
fn row(name: &str, encodes: &mut [Duration], decodes: &[Duration]) -> Vec<String> {
    encodes.sort_unstable();
    let micros = |duration: Duration| duration.as_secs_f64() * 1e6;
    let mean = |times: &[Duration]| times.iter().map(|&time| micros(time)).sum::<f64>() / times.len() as f64;
    let at_percent = |percent: usize| micros(encodes[(encodes.len() - 1) * percent / 100]);
    vec![
        name.to_string(),
        encodes.len().to_string(),
        format!("{:.1}", mean(encodes)),
        format!("{:.1}", at_percent(MEDIAN_PERCENT)),
        format!("{:.1}", at_percent(TAIL_PERCENT)),
        format!("{:.1}", micros(encodes[encodes.len() - 1])),
        format!("{:.1}", mean(decodes)),
    ]
}
