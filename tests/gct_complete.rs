//! Complete tests: everything the measurements run on, plus a moderate
//! sample from a second seed base the measurements never see, plus the
//! checkerboards; and every family turned each way round, taking about
//! as many bits.
//!
//! `cargo test --release --test gct_complete -- --ignored`

mod common;

use tilesim::gct::grammar::bit_stream::BitStream;
use tilesim::gct::Gct;
use tilesim::sample_generators::checkerboards::checkerboards;
use tilesim::sample_generators::{families, grown, one_laid_out, sample_seed, HowMany, PLANS, SHAPES};
use tilesim::Bitmap;
use common::check;

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
        let seed = sample_seed(shape.name).wrapping_add(SECOND_SEED_OFFSET);
        for (case, bitmap) in grown(seed, shape.density, shape.cluster, SECOND_SAMPLE_EACH).enumerate() {
            check(&bitmap, &format!("{}, second seed base, case {case}", shape.name));
        }
    }
    for plan in &PLANS {
        let seed = sample_seed(plan.name).wrapping_add(SECOND_SEED_OFFSET);
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

/// Quarter turns in a whole turn.
const QUARTER_TURNS: usize = 4;

/// How far, in percent, a family's bits may move when every bitmap in it
/// is turned a quarter, a half or three quarters. gct is not the same
/// every way round -- Morton order halves top and bottom first, copies
/// read up and to the left, the last pass codes rows top down -- so a
/// turned bitmap's tiles, copies and contexts differ. At the timed
/// counts, over four seeds, no family moved more than 0.84%; this keeps
/// more than twice that as margin for any seed, and still catches a
/// bias for one orientation, like the 10% the saved horizontal-streaks
/// bitmap gains turned.
const MOST_TURNED_DRIFT_PERCENT: f64 = 2.0;

/// `bitmap` turned a quarter clockwise.
fn turned_a_quarter(bitmap: &Bitmap) -> Bitmap {
    let mut turned = Bitmap::new();
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if bitmap.get(x, y) {
                turned.set(u8::MAX - y, x);
            }
        }
    }
    turned
}

/// Every family, at its `timed` count, takes about as many bits turned
/// any way round: each turn's total within [`MOST_TURNED_DRIFT_PERCENT`]
/// of the family's total as drawn.
#[test]
#[ignore]
fn turned_bitmaps_take_about_as_many_bits() {
    let (mut gct, mut stream) = (Gct::new(), BitStream::default());
    for (family, maps) in families(HowMany::Timed) {
        let mut bits_by_turn = [0; QUARTER_TURNS];
        for bitmap in &maps {
            let mut turned = bitmap.clone();
            for bits in &mut bits_by_turn {
                gct.encode(&turned, &mut stream);
                *bits += stream.len();
                turned = turned_a_quarter(&turned);
            }
        }
        let as_drawn = bits_by_turn[0];
        for (quarter_turns, &bits) in bits_by_turn.iter().enumerate().skip(1) {
            let drift = (bits as f64 / as_drawn as f64 - 1.0) * 100.0;
            assert!(
                drift.abs() <= MOST_TURNED_DRIFT_PERCENT,
                "{family}: turned {} degrees, {bits} bits against {as_drawn} as drawn ({drift:+.2}%)",
                quarter_turns * 90
            );
        }
    }
}
