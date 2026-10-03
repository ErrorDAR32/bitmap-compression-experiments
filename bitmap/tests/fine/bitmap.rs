//! The bitmap: its cells, squares, rectangles and circles.
//!
//! `cargo test`

use bitmap::{Bitmap, HEIGHT, WIDTH};

/// A bitmap is empty until a cell is set, and again once it is unset.
#[test]
fn empty_until_a_cell_is_set() {
    let mut bitmap = Bitmap::new();
    assert!(bitmap.is_empty());
    bitmap.set(255, 255);
    assert!(!bitmap.is_empty());
    bitmap.unset(255, 255);
    assert!(bitmap.is_empty());
}

/// Square fills agree with the same squares drawn cell by cell.
#[test]
fn squares_agree_with_their_cells() {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(8, 8, 15, 15);
    bitmap.set_rect(16, 0, 19, 3);
    bitmap.set(24, 4);
    let places: Vec<usize> = bitmap.set_cells_in_tile((16, 0), 8).collect();
    assert_eq!(places, (0..16).collect::<Vec<_>>(), "the 4x4 at (16, 0) is the first 16 of its 8x8");
    assert_eq!(bitmap.set_cells_in_tile((24, 4), 1).collect::<Vec<_>>(), vec![0]);
    let mut placed = Bitmap::new();
    for place in bitmap.set_cells_in_tile((0, 0), 32) {
        placed.set_in_tile((0, 0), place);
    }
    assert!((0..32).all(|y| (0..32).all(|x| placed.get(x, y) == bitmap.get(x, y))));
    let mut filled = Bitmap::new();
    filled.set_tile((8, 8), 8);
    filled.set_tile((16, 0), 4);
    filled.set_tile((24, 4), 1);
    assert!((0..=u8::MAX).all(|y| (0..=u8::MAX).all(|x| filled.get(x, y) == bitmap.get(x, y))));
}

/// A new bitmap has nothing set.
#[test]
fn starts_empty() {
    let bitmap = Bitmap::new();
    assert_eq!(bitmap.count_set(), 0);
    assert!(!bitmap.get(0, 0));
    assert!(!bitmap.get(255, 255));
}

/// Setting then unsetting one cell leaves it, and the count, as before.
#[test]
fn set_and_unset_single_bit() {
    let mut bitmap = Bitmap::new();
    bitmap.set(10, 20);
    assert!(bitmap.get(10, 20));
    assert_eq!(bitmap.count_set(), 1);
    bitmap.unset(10, 20);
    assert!(!bitmap.get(10, 20));
    assert_eq!(bitmap.count_set(), 0);
}

/// A rectangle includes both corners, whichever way round they are
/// named.
#[test]
fn rect_is_inclusive_and_order_independent() {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(5, 5, 2, 2);
    assert_eq!(bitmap.count_set(), 16);
    assert!((2..=5).all(|y| (2..=5).all(|x| bitmap.get(x, y))));
    bitmap.unset_rect(2, 2, 5, 5);
    assert_eq!(bitmap.count_set(), 0);
}

/// A rectangle hanging off the edge is clamped to the bitmap.
#[test]
fn rect_clamps_to_bounds() {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(-10, -10, 1, 1);
    assert_eq!(bitmap.count_set(), 4);
}

/// A circle holds its centre and cells at its radius, not its bounding
/// box's corners.
#[test]
fn circle_includes_centre_and_excludes_far_corners() {
    let mut bitmap = Bitmap::new();
    bitmap.set_circle(128, 128, 5);
    assert!(bitmap.get(128, 128));
    assert!(bitmap.get(133, 128));
    assert!(!bitmap.get(134, 128));
    assert!(!bitmap.get(133, 133));
}

/// Unsetting a circle clears what setting it set.
#[test]
fn unset_circle_clears_previously_set_bits() {
    let mut bitmap = Bitmap::new();
    bitmap.set_circle(50, 50, 10);
    assert!(bitmap.count_set() > 0);
    bitmap.unset_circle(50, 50, 10);
    assert_eq!(bitmap.count_set(), 0);
}

/// Resetting clears every cell.
#[test]
fn reset_clears_everything() {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(0, 0, 255, 255);
    assert_eq!(bitmap.count_set() as usize, WIDTH * HEIGHT);
    bitmap.reset();
    assert_eq!(bitmap.count_set(), 0);
}
