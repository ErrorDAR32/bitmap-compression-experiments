//! Adversarial bitmaps: a search for the bitmaps gct does worst on --
//! what it costs beyond the raw cells -- four searches at once, one a
//! core, each from its own seed. See `docs/testing_protocol.md`.
//!
//! Each search has two stages, each a simulated annealing (`anneal.rs`)
//! over structure-aware changes (`moves.rs`):
//!
//! 1. One 64x64 window, in an otherwise clear bitmap, from a clear start
//!    and from a noisy one: every change lands where it counts, and gct
//!    reads a quadtree, so what is bad in a window is bad anywhere.
//! 2. The whole plane: filled with the best window's sixteen variants
//!    (`plane.rs`), from noise, and from the worst bitmap recorded so
//!    far, carried on from where the last run left it.
//!
//! The worst plane of all four is recorded (`record.rs`) when it beats
//! the record, and the recorded bitmap must still round trip.
//!
//! `cargo test --release --test gct_adversarial_generator -- --ignored --nocapture`

#[path = "../common/mod.rs"]
mod common;
mod anneal;
mod moves;
mod objectives;
mod plane;
mod record;
mod rng;

use anneal::{anneal, Found};
use bitmap::gct::tile::Tile;
use bitmap::samples::sample_seed;
use bitmap::table::Table;
use bitmap::Bitmap;
use objectives::score;
use rng::Rng;
use std::thread;

/// The searched window: a 64x64, the top left one.
const WINDOW: Tile = Tile { level: 2, x: 0, y: 0 };

/// Changes tried on a window, from each start.
const WINDOW_ITERATIONS: u64 = 400;

/// Changes tried on the whole plane, from each start.
const PLANE_ITERATIONS: u64 = 100;

/// One in this many of a noisy start's cells is set: half, the most
/// disordered.
const NOISE_DENSITY_DIVISOR: u64 = 2;

/// Searches run at once, one a core.
const SEARCHES: u64 = 4;

/// What the record is kept under.
const RECORD: &str = "gct_against_raw";

/// `area` filled with noise, the rest clear.
fn noise(rng: &mut Rng, area: Tile) -> Bitmap {
    let mut bitmap = Bitmap::new();
    let (left, top, right, bottom) = area.cell_rect();
    for y in top..=bottom {
        for x in left..=right {
            if rng.below(NOISE_DENSITY_DIVISOR) == 0 {
                bitmap.set(x, y);
            }
        }
    }
    bitmap
}

/// The best of `starts`, each annealed in `area`, and which start it
/// came from.
fn best_of(starts: Vec<(&'static str, Bitmap)>, area: Tile, iterations: u64, rng: &mut Rng) -> (Found, &'static str) {
    starts
        .into_iter()
        .map(|(from, start)| (anneal(start, area, iterations, rng), from))
        .max_by_key(|(found, _)| found.score.gap)
        .unwrap()
}

/// What one search found, and which start each stage's best came from.
struct Outcome {
    /// The worst found in the window stage, searching one small tile.
    window: Found,
    /// Which start the window stage's worst came from.
    window_from: &'static str,
    /// The worst found over the whole bitmap.
    worst: Found,
    /// Which start that came from.
    worst_from: &'static str,
}

/// One whole search, from its own seed.
fn search(seed: u64, recorded: Option<Bitmap>) -> Outcome {
    let mut rng = Rng::new(seed);
    let window_starts = vec![("clear", Bitmap::new()), ("noise", noise(&mut rng, WINDOW))];
    let (window, window_from) = best_of(window_starts, WINDOW, WINDOW_ITERATIONS, &mut rng);

    let whole = Tile::whole_bitmap();
    let mut plane_starts =
        vec![("window variants", plane::fill_the_plane(&window.bitmap, WINDOW)), ("noise", noise(&mut rng, whole))];
    plane_starts.extend(recorded.map(|bitmap| ("record", bitmap)));
    let (worst, worst_from) = best_of(plane_starts, whole, PLANE_ITERATIONS, &mut rng);
    Outcome { window, window_from, worst, worst_from }
}

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
                scope.spawn(move || search(seed.wrapping_add(index), recorded))
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
