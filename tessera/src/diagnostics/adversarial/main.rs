//! Adversarial bitmaps against the raw cells: a search for the bitmaps
//! Tessera does worst on -- what it costs beyond the raw cells -- four
//! searches at once, one a core, each from its own seed. The search
//! itself is the library's (`tessera::diagnostics::adversarial`); this scores it. See
//! `docs/testing_protocol.md`.
//!
//! The worst plane of all four is kept when it beats the worst kept so
//! far, and the bitmap kept must still round trip. What each search
//! found, and the worst kept, are printed and kept in
//! `transient_data/measurements/adversarial.csv`.
//!
//! ```text
//! cargo run --release --bin adversarial
//! cargo run --release --bin adversarial -- 4000
//! ```
//!
//! The argument, if given, is how many changes each search tries on the
//! whole plane from each start.
//!
//! `save` keeps a worst bitmap as a named bitmap instead: copied to
//! `external_benchmarks/adversarial/saved/`, where no search replaces
//! it, under a name saying what it is, with a line describing it and the
//! worst bitmap's own notes (what it scored) as its comment lines.
//!
//! ```text
//! cargo run --release --bin adversarial -- save \
//!     against_zstd3 near_repeated_half_vs_zstd3 "bottom half a near repeat of the top, ..."
//! ```

#![warn(missing_docs, clippy::missing_docs_in_private_items)]

use tessera::diagnostics::adversarial::{worst, search_at_once, Effort, Score, SEARCHES_AT_ONCE};
use tessera::diagnostics::examination::Examination;
use tessera::encode;
use tessera::BitStream;
use tessera::tile::{cells_in_tile, Tile};
use tessera::Tessera;
use tessera::corpus::corpus_seed;
use tessera::transient_data;
use utilities::table::report::Report;
use utilities::table::Table;
use bitmap::Bitmap;

/// The command that saves a worst bitmap as a named bitmap.
const SAVE: &str = "save";

/// Copies the worst bitmap named by the first argument to the saved bitmap
/// named by the second, described by the third.
fn save(arguments: &[String]) {
    let [from, name, description] = arguments else {
        panic!("usage: adversarial save <worst> <saved name> <description>");
    };
    let bitmap = worst::read(from).unwrap_or_else(|| panic!("no worst bitmap named {from}"));
    let mut notes = vec![format!("{name}: {description}")];
    notes.extend(worst::notes_from(&worst::path(from)).into_iter().map(|note| format!("from worst {note}")));
    worst::save(name, &bitmap, &notes);
    let mut table =
        Table::new(&["saved", "from worst", "cells\nset", "file", "description"]).left_aligned(&["from worst", "file", "description"]);
    let file = worst::saved_path(name);
    let file = file.strip_prefix(env!("CARGO_MANIFEST_DIR")).unwrap_or(&file);
    table.row(&[name, from, &bitmap.count_set().to_string(), &file.display().to_string(), description]);
    table.print();
}

/// What the worst bitmap is kept under.
const WORST: &str = "against_raw";

/// What the search maximizes. Not Tessera's bits alone -- noise maximizes
/// those for any encoder, and says nothing -- but what Tessera costs beyond
/// the raw cells of `area`, the area searched.
fn score(bitmap: &Bitmap, area: Tile) -> Score {
    let tessera_bits = encode(bitmap).len() as u64;
    Score { gap: tessera_bits as i64 - cells_in_tile(area.level) as i64, tessera_bits }
}

/// Runs the searches in parallel, keeps the worst bitmap if it beats the
/// one kept, and reports what each search found and what the worst kept
/// is now.
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().is_some_and(|command| command == SAVE) {
        return save(&arguments[1..]);
    }
    let seed = corpus_seed();
    let effort = Effort::from_arguments();
    let kept = worst::read(WORST);
    let outcomes = search_at_once(seed, kept.clone(), effort, &|| score);

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
    let mut report = Report::new("adversarial", "cargo run --release --bin adversarial");
    report.note(format!(
        "{SEARCHES_AT_ONCE} searches, {} changes a window start, {} a plane start; gap: Tessera's bits less the raw cells searched",
        effort.window, effort.plane
    ));
    report.add("the searches", table);

    let worst = outcomes.iter().map(|outcome| &outcome.worst).max_by_key(|found| found.score.gap).expect("a search");
    let kept_gap = kept.map(|bitmap| score(&bitmap, Tile::WHOLE_BITMAP).gap);
    let beaten = kept_gap.is_none_or(|gap| worst.score.gap > gap);
    if beaten {
        worst::write(WORST, &worst.bitmap, &format!("{WORST}: gap {} bits", worst.score.gap));
    }
    let bitmap = worst::read(WORST).expect("kept");
    let examined = Examination::of(&mut Tessera::new(), &mut BitStream::default(), &mut Bitmap::new(), &bitmap);
    assert_eq!(examined.first_difference, None, "{WORST} does not round trip");

    let kept_gap_now = if beaten { worst.score.gap } else { kept_gap.expect("a worst bitmap not beaten is there") };
    let mut table = Table::new(&["worst", "worst gap\nthis run", "kept gap\nbefore", "kept gap\nnow", "replaced"]);
    table.row(&[
        WORST.to_string(),
        worst.score.gap.to_string(),
        kept_gap.map_or("none".to_string(), |gap| gap.to_string()),
        kept_gap_now.to_string(),
        if beaten { "yes" } else { "no" }.to_string(),
    ]);
    report.add("the worst kept, which round trips", table);
    transient_data::publish(report);
}
