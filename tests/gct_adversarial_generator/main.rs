//! Adversarial bitmaps: a search for the bitmaps gct does worst on,
//! against dsrn and against the raw cells. See `docs/testing_protocol.md`.
//!
//! Two stages, each a simulated annealing (`anneal.rs`) over
//! structure-aware changes (`moves.rs`):
//!
//! 1. One 64x64 window, in an otherwise clear bitmap, from a clear start
//!    and from a noisy one: every change lands where it counts, and
//!    both encoders are quadtrees, so what is bad in a window is bad
//!    anywhere.
//! 2. The whole plane, filled with the best window's sixteen variants
//!    (`plane.rs`), and, from the worst bitmap recorded so far, carried
//!    on from where the last run left it.
//!
//! The worst plane is recorded (`record.rs`) when it beats the record,
//! and every recorded bitmap must still round trip.
//!
//! `cargo test --release --test gct_adversarial_generator -- --ignored --nocapture`
//! -- about nine minutes: scoring takes about 70 ms, a clear window no
//! less than a full plane.

#[path = "../common/mod.rs"]
mod common;
mod anneal;
mod moves;
mod objectives;
mod plane;
mod record;
mod rng;

use anneal::anneal;
use bitmap::gct::tile::Tile;
use bitmap::samples::sample_seed;
use bitmap::table::Table;
use bitmap::Bitmap;
use objectives::{Scorer, OBJECTIVES};
use rng::Rng;

/// The searched window: a 64x64, the top left one.
const WINDOW: Tile = Tile { level: 2, x: 0, y: 0 };

/// Changes tried on a window, from each start.
const WINDOW_ITERATIONS: u64 = 1500;

/// Changes tried on the whole plane, from each start: fewer, since the
/// window's best already makes up the plane.
const PLANE_ITERATIONS: u64 = 150;

/// The share of a noisy start's cells set: half, the most disordered.
const NOISE_DENSITY_DIVISOR: u64 = 2;

fn noisy_window(rng: &mut Rng) -> Bitmap {
    let mut bitmap = Bitmap::new();
    let (left, top, right, bottom) = WINDOW.cell_rect();
    for y in top..=bottom {
        for x in left..=right {
            if rng.below(NOISE_DENSITY_DIVISOR) == 0 {
                bitmap.set(x, y);
            }
        }
    }
    bitmap
}

#[test]
#[ignore]
fn search_adversarial_bitmaps() {
    let mut rng = Rng::new(sample_seed("adversarial search"));
    let mut scorer = Scorer::new();
    let mut table = Table::new(&[
        "objective",
        "worst window\ngap, bits",
        "worst plane\ngap, bits",
        "gct\nbits",
        "dsrn\nbits",
        "record",
    ]);

    for objective in OBJECTIVES {
        let windows = [Bitmap::new(), noisy_window(&mut rng)]
            .map(|start| anneal(start, WINDOW, objective, WINDOW_ITERATIONS, &mut rng, &mut scorer));
        let window = windows.into_iter().max_by_key(|found| found.score.gap).unwrap();

        let recorded = record::read(objective.name());
        let mut starts = vec![plane::fill_the_plane(&window.bitmap, WINDOW)];
        starts.extend(recorded.clone());
        let whole = Tile::whole_bitmap();
        let planes: Vec<_> = starts
            .into_iter()
            .map(|start| anneal(start, whole, objective, PLANE_ITERATIONS, &mut rng, &mut scorer))
            .collect();
        let worst = planes.into_iter().max_by_key(|found| found.score.gap).unwrap();

        let record_gap = recorded.map(|bitmap| scorer.score(objective, &bitmap, whole).gap);
        let beaten = record_gap.is_none_or(|gap| worst.score.gap > gap);
        if beaten {
            let note = format!("{}: gap {} bits, gct {} bits", objective.name(), worst.score.gap, worst.score.gct_bits);
            record::write(objective.name(), &worst.bitmap, &note);
        }
        let dsrn_bits = worst.score.dsrn_bits.unwrap_or_else(|| scorer.dsrn_bits(&worst.bitmap));
        table.row(&[
            objective.name().to_string(),
            window.score.gap.to_string(),
            worst.score.gap.to_string(),
            worst.score.gct_bits.to_string(),
            dsrn_bits.to_string(),
            match (beaten, record_gap) {
                (true, Some(gap)) => format!("new, was {gap}"),
                (true, None) => "new".to_string(),
                (false, Some(gap)) => format!("kept, {gap}"),
                (false, None) => unreachable!(),
            },
        ]);
        common::check(&record::read(objective.name()).expect("just recorded"), objective.name());
    }
    println!();
    table.print();
}
