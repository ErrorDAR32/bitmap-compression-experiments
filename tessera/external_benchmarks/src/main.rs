//! Tessera against existing bitmap compressors -- CCITT Group 4, JBIG
//! (jbigkit) and zstd -- on the same large sample the diagnostics tool's `timing` uses:
//! every generator, `sample_generators::TIMING_PER_GENERATOR` distinct bitmaps each. For
//! each family and codec: the mean encoded size, and the mean time to
//! encode and to decode, each bitmap encoded once and decoded once and
//! checked. Run in release, on its own -- no profiler, nothing else
//! busy:
//!
//! From `tessera/`, which the manifest path is relative to; the samples
//! use the seed every other run does (`transient_data/seed`):
//!
//! ```text
//! cargo run --release --manifest-path external_benchmarks/Cargo.toml
//! cargo run --release --manifest-path external_benchmarks/Cargo.toml -- 400
//! ```
//!
//! The argument, if given, is how many bitmaps each generator makes.
//! Every bitmap is turned into rows before anything is timed.

#![warn(missing_docs, clippy::missing_docs_in_private_items)]

use tessera::diagnostics::RAW_CELLS;
use tessera::sample_generators::{families, HowMany, TIMING_PER_GENERATOR};
use tessera::transient_data;
use utilities::table::report::Report;
use utilities::table::Table;
use bitmap::Bitmap;
use external_benchmarks::codecs::g4::G4;
use external_benchmarks::codecs::tessera::Tessera;
use external_benchmarks::codecs::jbig::Jbig;
use external_benchmarks::codecs::zstd::Zstd;
use external_benchmarks::codecs::Codec;
use external_benchmarks::rows::Rows;
use std::time::Instant;

/// zstd's default level, and a high one: how much more a general
/// compressor finds when given the time.
const ZSTD_LEVELS: [i32; 2] = [3, 19];

/// One family's bitmaps, named, each also in rows.
struct Family {
    /// What the family is called.
    name: String,
    /// Its bitmaps, each with its rows.
    bitmaps: Vec<(Bitmap, Rows)>,
}

impl Family {
    /// A family of these bitmaps.
    fn of(name: String, bitmaps: impl Iterator<Item = Bitmap>) -> Self {
        Self { name, bitmaps: bitmaps.map(|bitmap| { let rows = Rows::of(&bitmap); (bitmap, rows) }).collect() }
    }
}

/// One codec's totals over a family.
#[derive(Default, Clone, Copy)]
struct Totals {
    /// Bitmaps.
    count: usize,
    /// Encoded bits.
    bits: usize,
    /// Seconds encoding.
    encode: f64,
    /// Seconds decoding.
    decode: f64,
}

impl Totals {
    /// These and `other` together.
    fn plus(self, other: Totals) -> Totals {
        Totals {
            count: self.count + other.count,
            bits: self.bits + other.bits,
            encode: self.encode + other.encode,
            decode: self.decode + other.decode,
        }
    }
}

/// Builds the sample, runs every codec over every family, and prints a
/// table a family, then one for them all.
fn main() {
    let per_generator = std::env::args().nth(1).map_or(TIMING_PER_GENERATOR, |count| count.parse().expect("a count"));
    let families: Vec<Family> =
        families(HowMany::Each(per_generator)).into_iter().map(|(name, bitmaps)| Family::of(name, bitmaps.into_iter())).collect();
    let mut codecs: Vec<Box<dyn Codec>> = vec![Box::new(Tessera::new()), Box::new(G4::new()), Box::new(Jbig::new())];
    codecs.extend(ZSTD_LEVELS.map(|level| Box::new(Zstd::new(level)) as Box<dyn Codec>));

    // One pass of each codec over one bitmap first, so the first timed one
    // pays for nothing extra.
    let (bitmap, rows) = &families[0].bitmaps[0];
    for codec in codecs.iter_mut() {
        codec.encode(bitmap, rows);
        codec.decode();
    }

    let mut report = Report::new("external_benchmarks", "cargo run --release --manifest-path external_benchmarks/Cargo.toml");
    report.note(format!("{per_generator} bitmaps a generator"));
    let mut overall = vec![Totals::default(); codecs.len()];
    for family in &families {
        let totals: Vec<Totals> = codecs.iter_mut().map(|codec| run(codec.as_mut(), family)).collect();
        add_table(&mut report, &family.name, &codecs, &totals);
        for (sum, family_totals) in overall.iter_mut().zip(&totals) {
            *sum = sum.plus(*family_totals);
        }
    }
    add_table(&mut report, "all", &codecs, &overall);
    transient_data::publish(report);
}

/// Encodes and decodes every bitmap of `family` with `codec`, checking
/// each comes back whole, and totals the sizes and times.
fn run(codec: &mut dyn Codec, family: &Family) -> Totals {
    let mut totals = Totals::default();
    for (bitmap, rows) in &family.bitmaps {
        let start = Instant::now();
        codec.encode(bitmap, rows);
        totals.encode += start.elapsed().as_secs_f64();
        totals.bits += codec.encoded_bits();
        let start = Instant::now();
        codec.decode();
        totals.decode += start.elapsed().as_secs_f64();
        assert!(codec.decoded_matches(rows), "{} did not round trip a bitmap of {}", codec.name(), family.name);
        totals.count += 1;
    }
    totals
}

/// Adds one family's table to `report`: a row a codec, its mean bits
/// (and as a share of raw), encode and decode microseconds a bitmap.
fn add_table(report: &mut Report, name: &str, codecs: &[Box<dyn Codec>], totals: &[Totals]) {
    let mut table = Table::new(&["codec", "bits\na bitmap", "of the\nraw cells", "encode us\na bitmap", "decode us\na bitmap"]);
    for (codec, totals) in codecs.iter().zip(totals) {
        let count = totals.count as f64;
        let bits = totals.bits as f64 / count;
        table.row(&[
            codec.name().to_string(),
            format!("{bits:.0}"),
            format!("{:.1}%", 100.0 * bits / RAW_CELLS as f64),
            format!("{:.1}", totals.encode / count * 1e6),
            format!("{:.1}", totals.decode / count * 1e6),
        ]);
    }
    report.add(format!("{name}, {} bitmaps", totals[0].count), table);
}
