//! Adversarial bitmaps against the raw cells: a search for the bitmaps
//! Tessera does worst on -- what it costs beyond the raw cells -- four
//! searches at once, one a core, each from its own seed. The search
//! itself is the library's (`tessera::adversarial`); this scores it. See
//! `docs/testing_protocol.md`.
//!
//! The worst plane of all four is recorded when it beats the record,
//! and the recorded bitmap must still round trip. What each search found,
//! and the record, are printed and kept in
//! `docs/measurements/tessera_adversarial.csv`.
//!
//! ```text
//! cargo run --release --bin tessera_adversarial
//! cargo run --release --bin tessera_adversarial -- 4000
//! ```
//!
//! The argument, if given, is how many changes each search tries on the
//! whole plane from each start.
//!
//! `save` keeps a record as a named bitmap instead: the record's bitmap
//! copied to `external_benchmarks/adversarial/saved/`, where no search
//! replaces it, under a name saying what it is, with a line describing
//! it and the record's own notes (what it scored) as its comment lines.
//!
//! ```text
//! cargo run --release --bin tessera_adversarial -- save \
//!     tessera_against_zstd3 near_repeated_half_vs_zstd3 "bottom half a near repeat of the top, ..."
//! ```

#![warn(missing_docs, clippy::missing_docs_in_private_items)]

use tessera::adversarial::{record, search_at_once, Effort, Score, SEARCHES_AT_ONCE};
use tessera::diagnostics::examination::Examination;
use tessera::encode;
use tessera::grammar::bit_stream::BitStream;
use tessera::tile::{cells_in_tile, Tile};
use tessera::Tessera;
use tessera::sample_generators::sample_seed;
use tessera::table::report::Report;
use tessera::table::Table;
use tessera::Bitmap;

/// The command that saves a record as a named bitmap.
const SAVE: &str = "save";

/// Copies the record named by the first argument to the saved bitmap
/// named by the second, described by the third.
fn save(arguments: &[String]) {
    let [from, name, description] = arguments else {
        panic!("usage: tessera_adversarial save <record> <saved name> <description>");
    };
    let bitmap = record::read(from).unwrap_or_else(|| panic!("no record named {from}"));
    let mut notes = vec![format!("{name}: {description}")];
    notes.extend(record::notes_from(&record::path(from)).into_iter().map(|note| format!("from record {note}")));
    record::save(name, &bitmap, &notes);
    let mut table =
        Table::new(&["saved", "from record", "cells\nset", "file", "description"]).left_aligned(&["from record", "file", "description"]);
    let file = record::saved_path(name);
    let file = file.strip_prefix(env!("CARGO_MANIFEST_DIR")).unwrap_or(&file);
    table.row(&[name, from, &bitmap.count_set().to_string(), &file.display().to_string(), description]);
    table.print();
}

/// What the record is kept under.
const RECORD: &str = "tessera_against_raw";

/// What the search maximizes. Not Tessera's bits alone -- noise maximizes
/// those for any encoder, and says nothing -- but what Tessera costs beyond
/// the raw cells of `area`, the area searched.
fn score(bitmap: &Bitmap, area: Tile) -> Score {
    let tessera_bits = encode(bitmap).len() as u64;
    Score { gap: tessera_bits as i64 - cells_in_tile(area.level) as i64, tessera_bits }
}

/// Runs the searches in parallel, records the worst bitmap if it beats
/// the one on record, and reports what each search found and what the
/// record is now.
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().is_some_and(|command| command == SAVE) {
        return save(&arguments[1..]);
    }
    let seed = sample_seed();
    let effort = Effort::from_arguments();
    let recorded = record::read(RECORD);
    let outcomes = search_at_once(seed, recorded.clone(), effort, &|| score);

    let mut table = Table::new(&[
        "search",
        "worst window\ngap, bits",
        "window\nfrom",
        "worst plane\ngap, bits",
        "plane\nfrom",
        "Tessera\nbits",
    ])
    .left_aligned(&["window\nfrom", "plane\nfrom"]);
    for (index, outcome) in outcomes.iter().enumerate() {
        table.row(&[
            index.to_string(),
            outcome.window.score.gap.to_string(),
            outcome.window_from.to_string(),
            outcome.worst.score.gap.to_string(),
            outcome.worst_from.to_string(),
            outcome.worst.score.tessera_bits.to_string(),
        ]);
    }
    let mut report = Report::new("tessera_adversarial", "cargo run --release --bin tessera_adversarial");
    report.note(format!(
        "{SEARCHES_AT_ONCE} searches, {} changes a window start, {} a plane start; gap: Tessera's bits less the raw cells searched",
        effort.window, effort.plane
    ));
    report.add("the searches", table);

    let worst = outcomes.iter().map(|outcome| &outcome.worst).max_by_key(|found| found.score.gap).expect("a search");
    let record_gap = recorded.map(|bitmap| score(&bitmap, Tile::whole_bitmap()).gap);
    let beaten = record_gap.is_none_or(|gap| worst.score.gap > gap);
    if beaten {
        record::write(RECORD, &worst.bitmap, &format!("{RECORD}: gap {} bits", worst.score.gap));
    }
    let bitmap = record::read(RECORD).expect("recorded");
    let examined = Examination::of(&mut Tessera::new(), &mut BitStream::default(), &mut Bitmap::new(), &bitmap);
    assert_eq!(examined.first_difference, None, "{RECORD} does not round trip");

    let record_gap_now = if beaten { worst.score.gap } else { record_gap.expect("a record not beaten is there") };
    let mut table = Table::new(&["record", "worst gap\nthis run", "record's gap\nbefore", "record's gap\nnow", "replaced"]);
    table.row(&[
        RECORD.to_string(),
        worst.score.gap.to_string(),
        record_gap.map_or("none".to_string(), |gap| gap.to_string()),
        record_gap_now.to_string(),
        if beaten { "yes" } else { "no" }.to_string(),
    ]);
    report.add("the record, which round trips", table);
    report.publish();
}
