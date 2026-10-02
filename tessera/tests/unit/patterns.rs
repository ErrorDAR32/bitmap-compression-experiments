//! The pattern pyramid: homogeneous and copyable exactly when the cells
//! say so, whatever was built before.

use crate::patterns::{homogeneous_value_of, Patterns};
use crate::sample_generators::checkerboards::checkerboard;
use crate::sample_generators::{one_grown, seed_uncounted};
use crate::tile::{copy_offset, Tile, DIRECTIONS, FLOOR_LEVEL};
use bitmap::Bitmap;

/// `tile`'s cells, in reading order.
fn cells(bitmap: &Bitmap, tile: Tile) -> Vec<bool> {
    let ((left, top), side) = (tile.top_left_cell(), tile.side_in_cells());
    (0..side).flat_map(|dy| (0..side).map(move |dx| (left as usize + dx, top as usize + dy))).map(|(x, y)| bitmap.get(x as u8, y as u8)).collect()
}

/// A ragged bitmap and an odd checkerboard: the cases every test here runs on.
fn bitmaps() -> [Bitmap; 2] {
    [one_grown(seed_uncounted(), 0.20, 0.70), checkerboard(3)]
}

/// With the top-left quarter filled, that quarter is homogeneous and set,
/// the next one clear, and the whole bitmap neither.
#[test]
fn a_filled_quarter_is_homogeneous() {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(0, 0, 127, 127);
    let mut patterns = Patterns::new();
    patterns.build(&bitmap);
    let value = |tile| homogeneous_value_of(patterns.number(tile));
    assert_eq!(value(Tile { level: 1, x: 0, y: 0 }), Some(true));
    assert_eq!(value(Tile { level: 1, x: 1, y: 0 }), Some(false));
    assert_eq!(value(Tile::WHOLE_BITMAP), None);
}

/// Every tile, at every level, is homogeneous by its number exactly when
/// its cells all agree -- after another bitmap was built first.
#[test]
fn homogeneity_matches_the_cells() {
    let mut patterns = Patterns::new();
    patterns.build(&checkerboard(5));
    for bitmap in bitmaps() {
        patterns.build(&bitmap);
        for tile in (0..=FLOOR_LEVEL).flat_map(Tile::all_of_level) {
            let cells = cells(&bitmap, tile);
            let agree = cells.iter().all(|&cell| cell == cells[0]);
            assert_eq!(homogeneous_value_of(patterns.number(tile)), agree.then_some(cells[0]), "{tile:?}");
        }
    }
}

/// Every tile's copy source is the first offset, near before far, whose
/// tile holds the same cells, if any.
#[test]
fn copy_sources_agree_with_the_cells() {
    let mut patterns = Patterns::new();
    for bitmap in bitmaps() {
        patterns.build(&bitmap);
        for tile in (0..=FLOOR_LEVEL).flat_map(Tile::all_of_level) {
            let mine = cells(&bitmap, tile);
            let first_match = [false, true].into_iter().flat_map(|far| (0..DIRECTIONS).map(move |direction| (far, direction))).find(
                |&(far, direction)| tile.offset_by(copy_offset(far, direction)).is_some_and(|source| cells(&bitmap, source) == mine),
            );
            assert_eq!(patterns.copy_source(tile, patterns.number(tile)), first_match, "{tile:?}");
        }
    }
}
