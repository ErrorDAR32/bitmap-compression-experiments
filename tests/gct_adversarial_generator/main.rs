//! Adversarial bitmaps: a search for the bitmaps each encoder does worst
//! on -- gct against dsrn and against the raw cells, and dsrn against
//! gct and against the raw cells -- one search a core. See
//! `docs/testing_protocol.md`.
//!
//! Two stages, each a simulated annealing (`anneal.rs`) over
//! structure-aware changes (`moves.rs`):
//!
//! 1. One 64x64 window, in an otherwise clear bitmap, from a clear start
//!    and from a noisy one: every change lands where it counts, and
//!    both encoders are quadtrees, so what is bad in a window is bad
//!    anywhere.
//! 2. The whole plane: filled with the best window's sixteen variants
//!    (`plane.rs`), from noise, and from the worst bitmap recorded so
//!    far, carried on from where the last run left it.
//!
//! The worst plane is recorded (`record.rs`) when it beats the record,
//! and every recorded bitmap must still round trip.
//!
//! `cargo test --release --test gct_adversarial_generator -- --ignored --nocapture`
//! -- a few minutes: scoring takes tens of milliseconds an encoder, a
//! clear window no less than a full plane.

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
use objectives::{Encoder, Objective, Scorer, OBJECTIVES};
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

/// What one objective's search found.
struct Outcome {
    objective: Objective,
    window: Found,
    worst: Found,
    record_gap: Option<i64>,
    beaten: bool,
}

/// One objective's whole search, from its own seed; records what it
/// found if it beats the record.
fn search(objective: Objective, seed: u64) -> Outcome {
    let mut rng = Rng::new(seed);
    let mut scorer = Scorer::new();
    let whole = Tile::whole_bitmap();

    let window_starts = [Bitmap::new(), noise(&mut rng, WINDOW)];
    let window = window_starts
        .map(|start| anneal(start, WINDOW, objective, WINDOW_ITERATIONS, &mut rng, &mut scorer))
        .into_iter()
        .max_by_key(|found| found.score.gap)
        .unwrap();

    let recorded = record::read(objective.name());
    let mut plane_starts = vec![plane::fill_the_plane(&window.bitmap, WINDOW), noise(&mut rng, whole)];
    plane_starts.extend(recorded.clone());
    let worst = plane_starts
        .into_iter()
        .map(|start| anneal(start, whole, objective, PLANE_ITERATIONS, &mut rng, &mut scorer))
        .max_by_key(|found| found.score.gap)
        .unwrap();

    let record_gap = recorded.map(|bitmap| scorer.score(objective, &bitmap, whole).gap);
    let beaten = record_gap.is_none_or(|gap| worst.score.gap > gap);
    if beaten {
        let note = format!("{}: gap {} bits", objective.name(), worst.score.gap);
        record::write(objective.name(), &worst.bitmap, &note);
    }
    Outcome { objective, window, worst, record_gap, beaten }
}

#[test]
#[ignore]
fn search_adversarial_bitmaps() {
    let seed = sample_seed("adversarial search");
    let outcomes: Vec<Outcome> = thread::scope(|scope| {
        let searches: Vec<_> = OBJECTIVES
            .iter()
            .enumerate()
            .map(|(index, &objective)| scope.spawn(move || search(objective, seed.wrapping_add(index as u64))))
            .collect();
        searches.into_iter().map(|search| search.join().unwrap()).collect()
    });

    let mut scorer = Scorer::new();
    let mut table = Table::new(&[
        "objective",
        "worst window\ngap, bits",
        "worst plane\ngap, bits",
        "gct\nbits",
        "dsrn\nbits",
        "record",
    ]);
    for outcome in outcomes {
        let worst = &outcome.worst;
        let gct_bits = worst.score.gct_bits.unwrap_or_else(|| scorer.bits(Encoder::Gct, &worst.bitmap));
        let dsrn_bits = worst.score.dsrn_bits.unwrap_or_else(|| scorer.bits(Encoder::Dsrn, &worst.bitmap));
        table.row(&[
            outcome.objective.name().to_string(),
            outcome.window.score.gap.to_string(),
            worst.score.gap.to_string(),
            gct_bits.to_string(),
            dsrn_bits.to_string(),
            match (outcome.beaten, outcome.record_gap) {
                (true, Some(gap)) => format!("new, was {gap}"),
                (true, None) => "new".to_string(),
                (false, Some(gap)) => format!("kept, {gap}"),
                (false, None) => unreachable!("with no record, anything found is one"),
            },
        ]);
        let name = outcome.objective.name();
        common::check(&record::read(name).expect("recorded"), name);
    }
    println!();
    table.print();
}
