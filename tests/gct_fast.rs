//! Fast tests: a small sample from the seed in `tests/last_seed` --
//! every shape, sparse shape, plan and line set, at its `tested` count;
//! and each family, and the saved bitmaps, turned every way round.
//!
//! `cargo test --test gct_fast`

mod common;
mod turning;

use tilesim::adversarial::record;
use tilesim::sample_generators::{families, HowMany, LINE_SETS, PLANS, SHAPES, SPARSE};
use common::check;
use turning::check_turned_bits;

/// How far, in percent, a family's bits may move turned a quarter, a
/// half or three quarters, at the tested counts -- as few as 6 bitmaps a
/// family, so a total moves more than at the timed counts: over eleven
/// seeds the line sets moved up to 3.0%, the city plans 1.2%, the other
/// families and the saved bitmaps under 0.5%. This keeps margin for any
/// seed, and still catches a bias like the 10% the saved
/// horizontal-streaks bitmap once gained turned.
const MOST_TURNED_DRIFT_PERCENT: f64 = 5.0;

/// Every shape and sparse shape, at its `tested` count, passes every check in
/// `common::check`: covered, capped, costed as written, and decoded back.
#[test]
fn every_shape_round_trips() {
    for shape in SHAPES.iter().chain(&SPARSE) {
        for (case, bitmap) in shape.tested().enumerate() {
            check(&bitmap, &format!("{}, case {case}", shape.name));
        }
    }
}

/// Every city plan, at its `tested` count, passes every check in
/// `common::check`: covered, capped, costed as written, and decoded back.
#[test]
fn every_plan_round_trips() {
    for plan in &PLANS {
        for (case, bitmap) in plan.tested().enumerate() {
            check(&bitmap, &format!("{}, case {case}", plan.name));
        }
    }
}

/// Every line set, at its `tested` count, passes every check in
/// `common::check`: covered, capped, costed as written, and decoded back.
#[test]
fn every_line_set_round_trips() {
    for set in &LINE_SETS {
        for (case, bitmap) in set.tested().enumerate() {
            check(&bitmap, &format!("{}, case {case}", set.name));
        }
    }
}

/// Every family, at its `tested` count, and the saved adversarial
/// bitmaps take about as many bits turned any way round: each turn's
/// total within [`MOST_TURNED_DRIFT_PERCENT`] of the total as drawn.
#[test]
fn turned_bitmaps_take_about_as_many_bits() {
    let mut sets = families(HowMany::Tested);
    sets.push(("the saved adversarial bitmaps".to_string(), record::saved().into_iter().map(|(_, bitmap)| bitmap).collect()));
    check_turned_bits(sets, MOST_TURNED_DRIFT_PERCENT);
}
