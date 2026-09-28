//! Adversarial bitmaps against each external codec: for each of CCITT
//! G4, JBIG and zstd, four searches at once, one a core, for the bitmap
//! where gct's bits most exceed that codec's -- the library's search
//! (`bitmap::adversarial`), scored as gct's bits less the codec's. The
//! worst found for each codec is kept as a PBM image in
//! `testing/adversarial/`, replaced only when beaten, and carried on
//! from by the next run; every recorded bitmap is checked to round trip
//! through both gct and the codec. Then, for each record, both
//! encoders' times on it.
//!
//! From the repository root, in release:
//!
//! ```text
//! cargo run --release --manifest-path comparison/Cargo.toml --bin adversarial
//! cargo run --release --manifest-path comparison/Cargo.toml --bin adversarial -- 4000
//! ```
//!
//! The argument, if given, is how many changes each search tries on the
//! whole plane from each start: one long search settles deeper than many
//! short ones.

#![warn(missing_docs, clippy::missing_docs_in_private_items)]

use bitmap::adversarial::{record, search, Effort, Outcome, Score};
use bitmap::samples::sample_seed;
use bitmap::table::Table;
use bitmap::Bitmap;
use comparison::codecs::g4::G4;
use comparison::codecs::gct::Gct;
use comparison::codecs::jbig::Jbig;
use comparison::codecs::zstd::Zstd;
use comparison::codecs::Codec;
use comparison::rows::Rows;
use std::thread;
use std::time::Instant;

/// Searches run at once for each codec, one a core.
const SEARCHES: u64 = 4;

/// Times each record is encoded to time it: the median is kept.
const TIMINGS: usize = 21;

/// A codec to search against: what its record is kept under, and how to
/// make one.
struct Opponent {
    /// The record's name, `gct_against_` and the codec.
    record: &'static str,
    /// A fresh codec: each search gets its own.
    make: fn() -> Box<dyn Codec>,
}

/// Every codec searched against.
const OPPONENTS: [Opponent; 4] = [
    Opponent { record: "gct_against_g4", make: || Box::new(G4::new()) },
    Opponent { record: "gct_against_jbig", make: || Box::new(Jbig::new()) },
    Opponent { record: "gct_against_zstd3", make: || Box::new(Zstd::new(3)) },
    Opponent { record: "gct_against_zstd19", make: || Box::new(Zstd::new(19)) },
];

/// gct's bits and the codec's, on one bitmap.
fn bits(gct: &mut Gct, codec: &mut dyn Codec, bitmap: &Bitmap) -> (u64, u64) {
    let rows = Rows::of(bitmap);
    gct.encode(bitmap, &rows);
    codec.encode(bitmap, &rows);
    (gct.encoded_bits() as u64, codec.encoded_bits() as u64)
}

/// gct's bits less the codec's: what the search maximizes.
fn score(gct: &mut Gct, codec: &mut dyn Codec, bitmap: &Bitmap) -> Score {
    let (gct_bits, codec_bits) = bits(gct, codec, bitmap);
    Score { gap: gct_bits as i64 - codec_bits as i64, gct_bits }
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

/// Searches against every codec, records any new worst, and prints the
/// records with both encoders' bits and times.
fn main() {
    let seed = sample_seed("adversarial search against codecs");
    let mut effort = Effort::default();
    if let Some(plane) = std::env::args().nth(1) {
        effort.plane = plane.parse().expect("a number of changes");
    }
    let mut table = Table::new(&[
        "against",
        "worst gap\nthis run",
        "record\ngap",
        "gct\nbits",
        "codec\nbits",
        "gct\nencode us",
        "codec\nencode us",
    ]);
    for (index, opponent) in OPPONENTS.iter().enumerate() {
        let recorded = record::read(opponent.record);
        let outcomes: Vec<Outcome> = thread::scope(|scope| {
            let searches: Vec<_> = (0..SEARCHES)
                .map(|search_index| {
                    let recorded = recorded.clone();
                    let seed = seed.wrapping_add(index as u64 * SEARCHES + search_index);
                    scope.spawn(move || {
                        let (mut gct, mut codec) = (Gct::new(), (opponent.make)());
                        search(seed, recorded, effort, &mut |bitmap, _| score(&mut gct, codec.as_mut(), bitmap))
                    })
                })
                .collect();
            searches.into_iter().map(|search| search.join().expect("a search")).collect()
        });
        let worst = outcomes.iter().map(|outcome| &outcome.worst).max_by_key(|found| found.score.gap).expect("a search");

        let (mut gct, mut codec) = (Gct::new(), (opponent.make)());
        let record_gap = recorded.as_ref().map(|bitmap| score(&mut gct, codec.as_mut(), bitmap).gap);
        if record_gap.is_none_or(|gap| worst.score.gap > gap) {
            record::write(opponent.record, &worst.bitmap, &format!("{}: gct {} bits over {}", opponent.record, worst.score.gap, codec.name()));
        }

        // The record, whichever it is now: both encoders must give it
        // back, and both are timed on it.
        let bitmap = record::read(opponent.record).expect("recorded");
        let rows = Rows::of(&bitmap);
        for coder in [&mut gct as &mut dyn Codec, codec.as_mut()] {
            coder.encode(&bitmap, &rows);
            coder.decode();
            assert!(coder.decoded_matches(&rows), "{} does not round trip {}", coder.name(), opponent.record);
        }
        let (gct_bits, codec_bits) = bits(&mut gct, codec.as_mut(), &bitmap);
        let gct_micros = median_micros(|| gct.encode(&bitmap, &rows));
        let codec_micros = median_micros(|| codec.encode(&bitmap, &rows));
        table.row(&[
            codec.name(),
            worst.score.gap.to_string(),
            (gct_bits as i64 - codec_bits as i64).to_string(),
            gct_bits.to_string(),
            codec_bits.to_string(),
            format!("{gct_micros:.0}"),
            format!("{codec_micros:.0}"),
        ]);
    }
    println!();
    table.print();
}
