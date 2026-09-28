//! Adversarial bitmaps against the raw cells: a search for the bitmaps
//! gct does worst on -- what it costs beyond the raw cells -- four
//! searches at once, one a core, each from its own seed. The search
//! itself is the library's (`bitmap::adversarial`); this scores it. See
//! `docs/testing_protocol.md`.
//!
//! The worst plane of all four is recorded when it beats the record,
//! and the recorded bitmap must still round trip.
//!
//! ```text
//! cargo run --release --bin gct_adversarial
//! cargo run --release --bin gct_adversarial -- 4000
//! ```
//!
//! The argument, if given, is how many changes each search tries on the
//! whole plane from each start.

#![warn(missing_docs, clippy::missing_docs_in_private_items)]

use bitmap::adversarial::{record, search_at_once, Effort, Score};
use bitmap::diagnostics::examination::Examination;
use bitmap::gct::encode;
use bitmap::gct::grammar::bit_stream::BitStream;
use bitmap::gct::tile::{cells_in_tile, Tile};
use bitmap::gct::Workspace;
use bitmap::samples::sample_seed;
use bitmap::table::Table;
use bitmap::Bitmap;

/// What the record is kept under.
const RECORD: &str = "gct_against_raw";

/// What the search maximizes. Not gct's bits alone -- noise maximizes
/// those for any encoder, and says nothing -- but what gct costs beyond
/// the raw cells of `area`, the area searched.
fn score(bitmap: &Bitmap, area: Tile) -> Score {
    let gct_bits = encode(bitmap).len() as u64;
    Score { gap: gct_bits as i64 - cells_in_tile(area.level) as i64, gct_bits }
}

/// Runs the searches in parallel, prints what each found, and records
/// the worst bitmap if it beats the one on record.
fn main() {
    let seed = sample_seed("adversarial search");
    let effort = Effort::from_arguments();
    let recorded = record::read(RECORD);
    let outcomes = search_at_once(seed, recorded.clone(), effort, &|| score);

    let mut table = Table::new(&[
        "search",
        "worst window\ngap, bits",
        "window\nfrom",
        "worst plane\ngap, bits",
        "plane\nfrom",
        "gct\nbits",
    ]);
    for (index, outcome) in outcomes.iter().enumerate() {
        table.row(&[
            index.to_string(),
            outcome.window.score.gap.to_string(),
            outcome.window_from.to_string(),
            outcome.worst.score.gap.to_string(),
            outcome.worst_from.to_string(),
            outcome.worst.score.gct_bits.to_string(),
        ]);
    }
    println!();
    table.print();

    let worst = outcomes.iter().map(|outcome| &outcome.worst).max_by_key(|found| found.score.gap).unwrap();
    let record_gap = recorded.map(|bitmap| score(&bitmap, Tile::whole_bitmap()).gap);
    if record_gap.is_none_or(|gap| worst.score.gap > gap) {
        record::write(RECORD, &worst.bitmap, &format!("{RECORD}: gap {} bits", worst.score.gap));
        println!("  new record: {} bits over raw (was {record_gap:?})", worst.score.gap);
    } else {
        println!("  record kept: {} bits over raw", record_gap.unwrap());
    }
    let bitmap = record::read(RECORD).expect("recorded");
    let examined = Examination::of(&mut Workspace::new(), &mut BitStream::default(), &mut Bitmap::new(), &bitmap);
    assert_eq!(examined.first_difference, None, "{RECORD} does not round trip");
}
