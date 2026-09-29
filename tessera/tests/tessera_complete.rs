//! Complete tests: everything the measurements run on, plus a moderate
//! sample from a second seed base the measurements never see, plus the
//! checkerboards; and every family turned each way round, taking about
//! as many bits.
//!
//! `cargo test --release --test tessera_complete -- --ignored`

mod common;

mod turning;

use tessera::adversarial::record;
use tessera::sample_generators::checkerboards::checkerboards;
use tessera::sample_generators::{families, grown, one_laid_out, sample_seed, HowMany, PLANS, SHAPES};
use common::check;
use turning::check_turned_bits;

/// How far from the measured seeds the second sample starts.
const SECOND_SEED_OFFSET: u64 = 1_000_000;

/// How many of each shape and plan the second sample takes.
const SECOND_SAMPLE_EACH: u64 = 4;

/// Every bitmap of every family, at its `timed` count, passes every check in
/// `common::check`: covered, capped, costed as written, and decoded back.
#[test]
#[ignore]
fn every_family_round_trips() {
    for (family, maps) in families(HowMany::Timed) {
        for (case, bitmap) in maps.iter().enumerate() {
            check(bitmap, &format!("{family}, case {case}"));
        }
    }
}

/// A few bitmaps of every shape and plan from seeds the measurements
/// never use, and each passes every check in `common::check`: covered,
/// capped, costed as written, and decoded back.
#[test]
#[ignore]
fn a_second_seed_base_round_trips() {
    for shape in &SHAPES {
        let seed = sample_seed().wrapping_add(SECOND_SEED_OFFSET);
        for (case, bitmap) in grown(seed, shape.density, shape.cluster, SECOND_SAMPLE_EACH).enumerate() {
            check(&bitmap, &format!("{}, second seed base, case {case}", shape.name));
        }
    }
    for plan in &PLANS {
        let seed = sample_seed().wrapping_add(SECOND_SEED_OFFSET);
        for case in 0..SECOND_SAMPLE_EACH {
            check(&one_laid_out(seed + case, plan), &format!("{}, second seed base, case {case}", plan.name));
        }
    }
}

/// Every checkerboard of odd-sided squares passes every check in
/// `common::check`: covered, capped, costed as written, and decoded back.
#[test]
#[ignore]
fn every_checkerboard_round_trips() {
    for (square_side, bitmap) in checkerboards() {
        check(&bitmap, &format!("checkerboard of {square_side}x{square_side} squares"));
    }
}

/// How far, in percent, a family's bits may move turned a quarter, a
/// half or three quarters, at the timed counts: more than twice what any
/// seed's families have moved, and still far under the bias the saved
/// horizontal-streaks bitmap once had for one orientation, when residual
/// blocks were counted at a bit a cell.
const MOST_TURNED_DRIFT_PERCENT: f64 = 2.0;

/// Every family, at its `timed` count, and the saved adversarial
/// bitmaps take about as many bits turned any way round: each turn's
/// total within [`MOST_TURNED_DRIFT_PERCENT`] of the total as drawn.
#[test]
#[ignore]
fn turned_bitmaps_take_about_as_many_bits() {
    let mut sets = families(HowMany::Timed);
    sets.push(("the saved adversarial bitmaps".to_string(), record::saved().into_iter().map(|(_, bitmap)| bitmap).collect()));
    check_turned_bits(sets, MOST_TURNED_DRIFT_PERCENT);
}
