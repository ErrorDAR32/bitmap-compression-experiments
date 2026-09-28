//! Fine tests: one bitmap each, drawn by hand or grown from a fixed
//! seed, so a failure points at one small, known case. Each also pins
//! down something specific the bitmap is meant to exercise.
//!
//! `cargo test --test cgt_fine`

mod common;

use bitmap::cgt::pyramids::bound_tile_counts::BoundTileCounts;
use bitmap::cgt::pyramids::copyable::Copyable;
use bitmap::cgt::pyramids::homogeneity::Homogeneity;
use bitmap::cgt::pyramids::pyramid::{Pyramid, PyramidShape};
use bitmap::cgt::tile::Tile;
use bitmap::cgt::tree::node::{Node, Tree};
use bitmap::cgt::tree::stats::TreeStats;
use bitmap::cgt::{encode, greedy_tiler::greedy_tiler, tree};
use bitmap::samples::{one_grown, one_laid_out, PLANS};
use bitmap::Bitmap;
use common::check;

/// A seed for the grown and laid-out cases here, fixed so each fine
/// test always runs on the same one bitmap.
const FIXED_SEED: u64 = 7;

#[test]
fn generic_pyramid_propagates_its_action() {
    let shape = PyramidShape { arity: 4, coarsest_level: 0, finest_level: 2, element_bits: 8 };
    let mut pyramid = Pyramid::new(shape);
    for tile in pyramid.tiles_of_level(2).collect::<Vec<_>>() {
        pyramid.set(tile, 1);
    }
    pyramid.propagate(|children| children.iter().sum());
    assert_eq!(pyramid.get(Tile { level: 1, x: 1, y: 1 }), 4);
    assert_eq!(pyramid.get(Tile::whole_bitmap()), 16);
}

#[test]
fn homogeneity_pyramid_sees_a_filled_quarter() {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(0, 0, 127, 127);
    let homogeneity = Pyramid::homogeneity(&bitmap);
    assert_eq!(homogeneity.homogeneous_value(Tile { level: 1, x: 0, y: 0 }), Some(true));
    assert_eq!(homogeneity.homogeneous_value(Tile { level: 1, x: 1, y: 0 }), Some(false));
    assert_eq!(homogeneity.homogeneous_value(Tile::whole_bitmap()), None);
}

#[test]
fn all_clear_is_one_tile_in_six_bits() {
    let bitmap = Bitmap::new();
    // leaf + bind + 3 resolution bits (depth 0, a tile) + 1 value bit
    assert_eq!(encode(&bitmap).len(), 6);
    assert_eq!(tree(&bitmap).node(Tile::whole_bitmap()), Node::Complex { depth: 0, masking: false });
    assert_eq!(Vec::<Pyramid>::bound_tile_counts(&greedy_tiler(&bitmap)).under(Tile::whole_bitmap(), 0), 1);
    check(&bitmap, "all clear");
}

#[test]
fn all_set_round_trips() {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(0, 0, 255, 255);
    assert_eq!(encode(&bitmap).len(), 6);
    check(&bitmap, "all set");
}

#[test]
fn a_repeated_quarter_is_a_near_copy() {
    let mut bitmap = Bitmap::new();
    bitmap.set_circle(64, 64, 40);
    bitmap.set_circle(192, 64, 40);
    let right_quarter = Tile { level: 1, x: 1, y: 0 };
    assert!(Pyramid::copyable(&bitmap).near_copyable(right_quarter));
    // DIRECTIONS[3] is the neighbour to the left.
    assert_eq!(tree(&bitmap).node(right_quarter), Node::Copied { far: false, direction: 3 });
    check(&bitmap, "a repeated quarter");
}

#[test]
fn rectangles_and_circles_round_trip() {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(10, 10, 40, 30);
    bitmap.set_rect(100, 3, 200, 90);
    bitmap.set_circle(180, 180, 25);
    bitmap.unset_circle(150, 50, 20);
    check(&bitmap, "rectangles and circles");
}

#[test]
fn one_city_round_trips_with_complex_tiles() {
    let bitmap = one_laid_out(FIXED_SEED, &PLANS[0]);
    assert!(TreeStats::of(&tree(&bitmap)).complex_tiles() > 0, "a city this regular forms complex tiles");
    check(&bitmap, "one city");
}

#[test]
fn one_ragged_bitmap_round_trips() {
    check(&one_grown(FIXED_SEED, 0.20, 0.70), "one middling ragged bitmap");
}
