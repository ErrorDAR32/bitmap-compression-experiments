//! Adversarial bitmaps: a search for the bitmaps gct does worst on --
//! what it costs beyond the raw cells -- four searches at once, one a
//! core, each from its own seed. The search itself is the library's
//! (`bitmap::adversarial`); this scores it. See
//! `docs/testing_protocol.md`.
//!
//! The worst plane of all four is recorded when it beats the record,
//! and the recorded bitmap must still round trip.
//!
//! `cargo test --release --test gct_adversarial_generator -- --ignored --nocapture`

#[path = "../common/mod.rs"]
mod common;
mod objectives;

use bitmap::adversarial::{record, search, Outcome};
use bitmap::gct::tile::Tile;
use bitmap::samples::sample_seed;
use bitmap::table::Table;
use objectives::score;
use std::thread;

/// Searches run at once, one a core.
const SEARCHES: u64 = 4;

/// What the record is kept under.
const RECORD: &str = "gct_against_raw";

/// Runs the searches in parallel, prints what each found, and records
/// the worst bitmap if it beats the one on record.
#[test]
#[ignore]
fn search_adversarial_bitmaps() {
    let seed = sample_seed("adversarial search");
    let recorded = record::read(RECORD);
    let outcomes: Vec<Outcome> = thread::scope(|scope| {
        let searches: Vec<_> = (0..SEARCHES)
            .map(|index| {
                let recorded = recorded.clone();
                scope.spawn(move || search(seed.wrapping_add(index), recorded, &mut |bitmap, area| score(bitmap, area)))
            })
            .collect();
        searches.into_iter().map(|search| search.join().unwrap()).collect()
    });

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
    common::check(&record::read(RECORD).expect("recorded"), RECORD);
}
