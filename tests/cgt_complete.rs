//! Complete tests: everything the measurements run on, plus a moderate
//! sample from a second seed base the measurements never see.
//!
//! `cargo test --release --test cgt_complete -- --ignored`

mod common;

use bitmap::samples::{every_family, grown, one_laid_out, sample_seed, PLANS, SHAPES};
use common::check;

/// How far from the measured seeds the second sample starts.
const SECOND_SEED_OFFSET: u64 = 1_000_000;

/// How many of each shape and plan the second sample takes.
const SECOND_SAMPLE_EACH: u64 = 4;

#[test]
#[ignore]
fn every_family_round_trips() {
    for (family, maps) in every_family() {
        for (case, bitmap) in maps.iter().enumerate() {
            check(bitmap, &format!("{family}, case {case}"));
        }
    }
}

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
