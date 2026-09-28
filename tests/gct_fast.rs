//! Fast tests: a small sample from the seed in `testing/last_seed` --
//! every shape, sparse shape, plan and line set, at its `tested` count.
//!
//! `cargo test --test gct_fast`

mod common;

use bitmap::samples::{LINE_SETS, PLANS, SHAPES, SPARSE};
use common::check;

#[test]
fn every_shape_round_trips() {
    for shape in SHAPES.iter().chain(&SPARSE) {
        for (case, bitmap) in shape.tested().enumerate() {
            check(&bitmap, &format!("{}, case {case}", shape.name));
        }
    }
}

#[test]
fn every_plan_round_trips() {
    for plan in &PLANS {
        for (case, bitmap) in plan.tested().enumerate() {
            check(&bitmap, &format!("{}, case {case}", plan.name));
        }
    }
}

#[test]
fn every_line_set_round_trips() {
    for set in &LINE_SETS {
        for (case, bitmap) in set.tested().enumerate() {
            check(&bitmap, &format!("{}, case {case}", set.name));
        }
    }
}
