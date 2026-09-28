//! Fast tests: a small sample from the seed in `testing/last_seed` --
//! every shape and every plan, at its `tested` count.
//!
//! `cargo test --test gct_fast`

mod common;

use bitmap::samples::{PLANS, SHAPES};
use common::check;

#[test]
fn every_shape_round_trips() {
    for shape in &SHAPES {
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
