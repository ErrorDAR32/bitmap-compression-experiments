//! Adversarial bitmaps against each external codec: for each of CCITT
//! G4, JBIG and zstd, four searches at once, one a core, for the bitmap
//! where Tessera's bits most exceed that codec's -- the library's search
//! (`tessera::adversarial`), scored as Tessera's bits less the codec's. The
//! worst found for each codec is kept as a PBM image in
//! `transient_data/records/`, replaced only when beaten, and carried on
//! from by the next run; every recorded bitmap is checked to round trip
//! through both Tessera and the codec. Then, for each record, both
//! encoders' times on it. The table -- a row a codec -- is printed and
//! kept in `transient_data/measurements/external_adversarial.csv`.
//!
//! From `tessera/`, which the manifest path is relative to, in release:
//!
//! ```text
//! cargo run --release --manifest-path external_benchmarks/Cargo.toml --bin adversarial
//! cargo run --release --manifest-path external_benchmarks/Cargo.toml --bin adversarial -- 4000
//! ```
//!
//! The argument, if given, is how many changes each search tries on the
//! whole plane from each start: one long search settles deeper than many
//! short ones.

#![warn(missing_docs, clippy::missing_docs_in_private_items)]

use tessera::adversarial::{record, search_at_once, Effort, Score, SEARCHES_AT_ONCE};
use tessera::sample_generators::sample_seed;
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

/// Times each record is encoded to time it: the median is kept.
const TIMINGS: usize = 21;

/// A codec to search against: what its record is kept under, and how to
/// make one.
struct Opponent {
    /// The record's name, `against_` and the codec.
    record: &'static str,
    /// A fresh codec: each search gets its own.
    make: fn() -> Box<dyn Codec>,
}

/// Every codec searched against.
const OPPONENTS: [Opponent; 4] = [
    Opponent { record: "against_g4", make: || Box::new(G4::new()) },
    Opponent { record: "against_jbig", make: || Box::new(Jbig::new()) },
    Opponent { record: "against_zstd3", make: || Box::new(Zstd::new(3)) },
    Opponent { record: "against_zstd19", make: || Box::new(Zstd::new(19)) },
];

/// Tessera's bits and the codec's, on one bitmap.
fn bits(tessera: &mut Tessera, codec: &mut dyn Codec, bitmap: &Bitmap) -> (u64, u64) {
    let rows = Rows::of(bitmap);
    tessera.encode(bitmap, &rows);
    codec.encode(bitmap, &rows);
    (tessera.encoded_bits() as u64, codec.encoded_bits() as u64)
}

/// Tessera's bits less the codec's: what the search maximizes.
fn score(tessera: &mut Tessera, codec: &mut dyn Codec, bitmap: &Bitmap) -> Score {
    let (tessera_bits, codec_bits) = bits(tessera, codec, bitmap);
    Score { gap: tessera_bits as i64 - codec_bits as i64, tessera_bits }
}

/// The median time, in microseconds, `encode` takes over [`TIMINGS`] runs.
fn median_micros(mut encode: impl FnMut()) -> f64 {
    let mut times: Vec<f64> = (0..TIMINGS)
        .map(|_| {
            let start = Instant::now();
            encode();
            start.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    times.sort_by(f64::total_cmp);
    times[TIMINGS / 2]
}

/// Searches against every codec, records any new worst, and reports the
/// records with both encoders' bits and times.
fn main() {
    let seed = sample_seed();
    let effort = Effort::from_arguments();
    let mut table = Table::new(&[
        "against",
        "worst gap\nthis run",
        "record's gap\nbefore",
        "record's gap\nnow",
        "record\nreplaced",
        "Tessera\nbits",
        "codec\nbits",
        "Tessera\nencode us",
        "codec\nencode us",
    ]);
    for (index, opponent) in OPPONENTS.iter().enumerate() {
        let recorded = record::read(opponent.record);
        let opponent_seed = seed.wrapping_add(index as u64 * SEARCHES_AT_ONCE);
        let outcomes = search_at_once(opponent_seed, recorded.clone(), effort, &|| {
            let (mut tessera, mut codec) = (Tessera::new(), (opponent.make)());
            move |bitmap: &Bitmap, _| score(&mut tessera, codec.as_mut(), bitmap)
        });
        let worst = outcomes.iter().map(|outcome| &outcome.worst).max_by_key(|found| found.score.gap).expect("a search");

        let (mut tessera, mut codec) = (Tessera::new(), (opponent.make)());
        let record_gap = recorded.as_ref().map(|bitmap| score(&mut tessera, codec.as_mut(), bitmap).gap);
        let beaten = record_gap.is_none_or(|gap| worst.score.gap > gap);
        if beaten {
            record::write(opponent.record, &worst.bitmap, &format!("{}: Tessera {} bits over {}", opponent.record, worst.score.gap, codec.name()));
        }

        // The record, whichever it is now: both encoders must give it
        // back, and both are timed on it.
        let bitmap = record::read(opponent.record).expect("recorded");
        let rows = Rows::of(&bitmap);
        for coder in [&mut tessera as &mut dyn Codec, codec.as_mut()] {
            coder.encode(&bitmap, &rows);
            coder.decode();
            assert!(coder.decoded_matches(&rows), "{} does not round trip {}", coder.name(), opponent.record);
        }
        let (tessera_bits, codec_bits) = bits(&mut tessera, codec.as_mut(), &bitmap);
        let tessera_micros = median_micros(|| tessera.encode(&bitmap, &rows));
        let codec_micros = median_micros(|| codec.encode(&bitmap, &rows));
        table.row(&[
            codec.name(),
            worst.score.gap.to_string(),
            record_gap.map_or("none".to_string(), |gap| gap.to_string()),
            (tessera_bits as i64 - codec_bits as i64).to_string(),
            if beaten { "yes" } else { "no" }.to_string(),
            tessera_bits.to_string(),
            codec_bits.to_string(),
            format!("{tessera_micros:.0}"),
            format!("{codec_micros:.0}"),
        ]);
    }
    let mut report = Report::new(
        "external_adversarial",
        "cargo run --release --manifest-path external_benchmarks/Cargo.toml --bin adversarial",
    );
    report.note(format!(
        "{SEARCHES_AT_ONCE} searches a codec, {} changes a window start, {} a plane start; gap: Tessera's bits less the codec's",
        effort.window, effort.plane
    ));
    report.note(format!("times: the median of {TIMINGS} encodings of the record"));
    report.add("the records, each round tripping through both", table);
    transient_data::publish(report);
}
