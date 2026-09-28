//! gct against existing bitmap compressors -- CCITT Group 4, JBIG
//! (jbigkit) and zstd -- on the same large sample `gct_timing` uses:
//! every generator, `BITMAPS_PER_GENERATOR` distinct bitmaps each. For
//! each family and codec: the mean encoded size, and the mean time to
//! encode and to decode, each bitmap encoded once and decoded once and
//! checked. Run in release, on its own -- no profiler, nothing else
//! busy:
//!
//! ```text
//! cd comparison
//! cargo run --release
//! cargo run --release -- 400
//! ```
//!
//! The argument, if given, is how many bitmaps each generator makes.
//! Every bitmap is turned into rows before anything is timed.

#![warn(missing_docs, clippy::missing_docs_in_private_items)]

use bitmap::samples::{LINE_SETS, PLANS, SHAPES, SPARSE};
use bitmap::Bitmap;
use comparison::codecs::g4::G4;
use comparison::codecs::gct::Gct;
use comparison::codecs::jbig::Jbig;
use comparison::codecs::zstd::Zstd;
use comparison::codecs::Codec;
use comparison::rows::{self, Rows};
use std::time::Instant;

/// Bitmaps each generator makes unless told otherwise: 20 generators,
/// so 2000 bitmaps.
const BITMAPS_PER_GENERATOR: u64 = 100;

/// zstd's default level, and a high one: how much more a general
/// compressor finds when given the time.
const ZSTD_LEVELS: [i32; 2] = [3, 19];

/// Raw bits a bitmap: one a cell.
const RAW_BITS: f64 = (rows::WIDTH * rows::HEIGHT) as f64;

/// One family's bitmaps, named, each also in rows.
struct Family {
    /// What the family is called.
    name: &'static str,
    /// Its bitmaps, each with its rows.
    bitmaps: Vec<(Bitmap, Rows)>,
}

impl Family {
    /// A family of these bitmaps.
    fn of(name: &'static str, bitmaps: impl Iterator<Item = Bitmap>) -> Self {
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
    let per_generator = std::env::args().nth(1).map_or(BITMAPS_PER_GENERATOR, |count| count.parse().expect("a count"));
    let families = [
        Family::of("grown like a blob", SHAPES.iter().flat_map(|shape| shape.take(per_generator))),
        Family::of("sparse", SPARSE.iter().flat_map(|shape| shape.take(per_generator))),
        Family::of("laid out like a city", PLANS.iter().flat_map(|plan| plan.take(per_generator))),
        Family::of("drawn with lines", LINE_SETS.iter().flat_map(|set| set.take(per_generator))),
    ];
    let mut codecs: Vec<Box<dyn Codec>> = vec![Box::new(Gct::new()), Box::new(G4::new()), Box::new(Jbig::new())];
    codecs.extend(ZSTD_LEVELS.map(|level| Box::new(Zstd::new(level)) as Box<dyn Codec>));

    // One pass of each codec over one bitmap first, so the first timed one
    // pays for nothing extra.
    let (bitmap, rows) = &families[0].bitmaps[0];
    for codec in codecs.iter_mut() {
        codec.encode(bitmap, rows);
        codec.decode();
    }

    let mut overall = vec![Totals::default(); codecs.len()];
    for family in &families {
        let totals: Vec<Totals> = codecs.iter_mut().map(|codec| run(codec.as_mut(), family)).collect();
        print_table(family.name, &codecs, &totals);
        for (sum, family_totals) in overall.iter_mut().zip(&totals) {
            *sum = sum.plus(*family_totals);
        }
    }
    print_table("all", &codecs, &overall);
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

/// Prints one family's table: a row a codec, its mean bits (and as a
/// share of raw), encode and decode microseconds a bitmap.
fn print_table(name: &str, codecs: &[Box<dyn Codec>], totals: &[Totals]) {
    println!("\n{name} ({} bitmaps)", totals[0].count);
    println!("  {:<22} {:>10} {:>8} {:>11} {:>11}", "codec", "bits", "of raw", "encode us", "decode us");
    for (codec, totals) in codecs.iter().zip(totals) {
        let count = totals.count as f64;
        let bits = totals.bits as f64 / count;
        println!(
            "  {:<22} {:>10.0} {:>7.1}% {:>11.1} {:>11.1}",
            codec.name(),
            bits,
            100.0 * bits / RAW_BITS,
            totals.encode / count * 1e6,
            totals.decode / count * 1e6
        );
    }
}
