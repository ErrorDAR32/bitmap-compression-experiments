//! Fine tests: one bitmap each, drawn by hand or grown from a fixed
//! seed, so a failure points at one small, known case. Each also pins
//! down something specific the bitmap is meant to exercise.
//!
//! `cargo test --test gct_fine`

mod common;

use bitmap::gct::pyramids::copyable::{matches_at, matching_direction, FAR_DISTANCE, FINEST_COPY_LEVEL, NEAR_DISTANCE};
use bitmap::gct::pyramids::homogeneity::Homogeneity;
use bitmap::gct::pyramids::pyramid::{Propagation, Pyramid, PyramidShape};
use bitmap::gct::tile::{directions, Tile, CELL_LEVEL};
use bitmap::gct::pyramids::tree::{Node, Tree};
use common::tree_stats::TreeStats;
use bitmap::gct::encode;
use bitmap::samples::checkerboards::checkerboard;
use bitmap::samples::{one_grown, one_laid_out, PLANS};
use bitmap::Bitmap;
use common::{check, tree_of};

/// A seed for the grown and laid-out cases here, fixed so each fine
/// test always runs on the same one bitmap.
const FIXED_SEED: u64 = 7;

/// A generic pyramid summing its children keeps every coarser level in
/// step through each set.
#[test]
fn generic_pyramid_propagates_every_set() {
    /// The rule: a tile holds the sum of its children.
    fn sum_of_children(_: u64, children: [u64; 4]) -> u64 {
        children.iter().sum()
    }
    let shape = PyramidShape { coarsest_level: 0, finest_level: 2, element_bits: 8 };
    let mut pyramid = Pyramid::with_propagation(shape, Propagation::OnEverySet(sum_of_children));
    pyramid.set(Tile { level: 2, x: 3, y: 3 }, 1);
    assert_eq!(pyramid.get(Tile { level: 1, x: 1, y: 1 }), 1);
    assert_eq!(pyramid.get(Tile::whole_bitmap()), 1);
    for tile in pyramid.tiles_of_level(2).collect::<Vec<_>>() {
        pyramid.set(tile, 1);
    }
    assert_eq!(pyramid.get(Tile { level: 1, x: 1, y: 1 }), 4);
    assert_eq!(pyramid.get(Tile::whole_bitmap()), 16);
}

/// The same rule applied in one sweep: sets change nothing else, until
/// the sweep brings every coarser level in step at once.
#[test]
fn generic_pyramid_propagates_in_one_sweep() {
    /// The rule: a tile holds the sum of its children.
    fn sum_of_children(_: u64, children: [u64; 4]) -> u64 {
        children.iter().sum()
    }
    let shape = PyramidShape { coarsest_level: 0, finest_level: 2, element_bits: 8 };
    let mut pyramid = Pyramid::with_propagation(shape, Propagation::InOneSweep(sum_of_children));
    for tile in pyramid.tiles_of_level(2).collect::<Vec<_>>() {
        pyramid.set(tile, 1);
    }
    assert_eq!(pyramid.get(Tile::whole_bitmap()), 0);
    pyramid.propagate();
    assert_eq!(pyramid.get(Tile { level: 1, x: 1, y: 1 }), 4);
    assert_eq!(pyramid.get(Tile::whole_bitmap()), 16);
}

/// With the top-left quarter filled, that quarter is homogeneous and set,
/// the next one homogeneous and clear, and the whole bitmap neither.
#[test]
fn homogeneity_pyramid_sees_a_filled_quarter() {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(0, 0, 127, 127);
    let homogeneity = Pyramid::homogeneity(&bitmap);
    assert_eq!(homogeneity.homogeneous_value(Tile { level: 1, x: 0, y: 0 }), Some(true));
    assert_eq!(homogeneity.homogeneous_value(Tile { level: 1, x: 1, y: 0 }), Some(false));
    assert_eq!(homogeneity.homogeneous_value(Tile::whole_bitmap()), None);
}

/// Every tile of a ragged bitmap and of an odd checkerboard, at every
/// level, against its own cells read one at a time.
#[test]
fn homogeneity_pyramid_matches_the_cells() {
    for bitmap in [one_grown(FIXED_SEED, 0.20, 0.70), checkerboard(3)] {
        let homogeneity = Pyramid::homogeneity(&bitmap);
        for level in 0..=CELL_LEVEL {
            for tile in Tile::all_of_level(level) {
                let (left, top, right, bottom) = tile.cell_rect();
                let first = bitmap.get(left, top);
                let agree = (top..=bottom).all(|y| (left..=right).all(|x| bitmap.get(x, y) == first));
                assert_eq!(homogeneity.homogeneous_value(tile), agree.then_some(first), "{tile:?}");
            }
        }
    }
}

/// An empty bitmap is one tile at the top, in 9 bits.
#[test]
fn all_clear_is_one_tile_in_nine_bits() {
    let bitmap = Bitmap::new();
    // 3 start level bits (0) + leaf + bind + 3 resolution bits (size
    // offset 0, a tile) + 1 value bit
    assert_eq!(encode(&bitmap).len(), 9);
    assert_eq!(tree_of(&bitmap).node(Tile::whole_bitmap()), Node::ComplexTile { size_offset: 0, masks: false });
    check(&bitmap, "all clear");
}

/// A full bitmap is 9 bits too, and passes every check.
#[test]
fn all_set_round_trips() {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(0, 0, 255, 255);
    assert_eq!(encode(&bitmap).len(), 9);
    check(&bitmap, "all set");
}

/// Every match of a ragged bitmap and of an odd checkerboard, at every
/// level a copy can be and every distance one is read from, against the
/// two tiles' cells read one at a time.
#[test]
fn matches_agree_with_the_cells() {
    for bitmap in [one_grown(FIXED_SEED, 0.20, 0.70), checkerboard(3)] {
        let homogeneity = Pyramid::homogeneity(&bitmap);
        for level in 0..=FINEST_COPY_LEVEL {
            for tile in Tile::all_of_level(level) {
                let mine = homogeneity.homogeneous_value(tile);
                for distance in [NEAR_DISTANCE, FAR_DISTANCE, 2 * FAR_DISTANCE] {
                    for direction in directions() {
                        let same = tile.neighbour_at(direction, distance).is_some_and(|other| {
                            let ((left, top, right, bottom), (x, y)) = (tile.cell_rect(), other.top_left_cell());
                            (top..=bottom).all(|row| {
                                (left..=right).all(|col| bitmap.get(col, row) == bitmap.get(x + (col - left), y + (row - top)))
                            })
                        });
                        let matched = matches_at(&homogeneity, &bitmap, tile, mine, direction, distance);
                        assert_eq!(matched, same, "{tile:?} {direction} {distance}");
                    }
                }
            }
        }
    }
}

/// The top-right quarter repeating the top-left one matches it, and is
/// placed as a near copy of it.
#[test]
fn a_repeated_quarter_is_a_near_copy() {
    let mut bitmap = Bitmap::new();
    bitmap.set_circle(64, 64, 40);
    bitmap.set_circle(192, 64, 40);
    let right_quarter = Tile { level: 1, x: 1, y: 0 };
    // DIRECTIONS[3] is the neighbour to the left.
    assert_eq!(matching_direction(&Pyramid::homogeneity(&bitmap), &bitmap, right_quarter, NEAR_DISTANCE), Some(3));
    assert_eq!(tree_of(&bitmap).node(right_quarter), Node::Copied { far: false, direction: 3, masks: false });
    check(&bitmap, "a repeated quarter");
}

/// Overlapping rectangles and circles, set and cleared, pass every
/// check.
#[test]
fn rectangles_and_circles_round_trip() {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(10, 10, 40, 30);
    bitmap.set_rect(100, 3, 200, 90);
    bitmap.set_circle(180, 180, 25);
    bitmap.unset_circle(150, 50, 20);
    check(&bitmap, "rectangles and circles");
}

/// A regular city forms complex tiles, and passes every check.
#[test]
fn one_city_round_trips_with_complex_tiles() {
    let bitmap = one_laid_out(FIXED_SEED, &PLANS[0]);
    assert!(TreeStats::of(&tree_of(&bitmap)).complex_tiles() > 0, "a city this regular forms complex tiles");
    check(&bitmap, "one city");
}

/// A middling, ragged bitmap passes every check.
#[test]
fn one_ragged_bitmap_round_trips() {
    check(&one_grown(FIXED_SEED, 0.20, 0.70), "one middling ragged bitmap");
}

/// A complex tile masks down to a point list where a lone cell sits.
#[test]
fn a_complex_tile_masks_a_lone_cell_down_to_a_point_list() {
    // The top-left 64x64: four 32x32s of 16x16 blocks, one block set in
    // each, a different one each time -- no 32x32 is homogeneous or a
    // copy of another, so the 64x64 is one complex tile at 16x16
    // resolution. One clear block holds a lone set cell, which no
    // resolution can say: that block is masked, down to the lone cell's
    // 8x8, said as a point list of one cell.
    /// The side of one block, in cells.
    const BLOCK: i64 = 16;
    let mut bitmap = Bitmap::new();
    for (block_x, block_y) in [(0, 0), (3, 0), (0, 3), (3, 3)] {
        let (x, y) = (block_x * BLOCK, block_y * BLOCK);
        bitmap.set_rect(x, y, x + BLOCK - 1, y + BLOCK - 1);
    }
    let lone_cell = Tile { level: 8, x: 21, y: 5 };
    bitmap.set(lone_cell.x, lone_cell.y);

    let written = tree_of(&bitmap);
    assert_eq!(written.node(lone_cell.ancestor(2)), Node::ComplexTile { size_offset: 2, masks: true });
    assert_eq!(written.node(lone_cell.ancestor(5)), Node::PointList);
    check(&bitmap, "a complex tile masking a lone cell");
}
