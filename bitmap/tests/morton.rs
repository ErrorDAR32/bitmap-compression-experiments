//! Morton order: numbered as drawn, and undone exactly.
//!
//! `cargo test`

use bitmap::morton::{morton_coordinates, morton_index};

/// The first sixteen cells are numbered as the module doc draws
/// them, and the last cell last.
#[test]
fn numbers_the_first_sixteen_as_drawn() {
    /// The module doc's drawing, row by row.
    const FIRST_SIXTEEN: [[usize; 4]; 4] = [[0, 1, 4, 5], [2, 3, 6, 7], [8, 9, 12, 13], [10, 11, 14, 15]];
    for (y, row) in FIRST_SIXTEEN.iter().enumerate() {
        for (x, &index) in row.iter().enumerate() {
            assert_eq!(morton_index(x as u8, y as u8), index);
        }
    }
    assert_eq!(morton_index(u8::MAX, u8::MAX), u16::MAX as usize);
}

/// Every coordinate pair comes back from its own Morton index.
#[test]
fn coordinates_invert_the_index() {
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            assert_eq!(morton_coordinates(morton_index(x, y)), (x, y));
        }
    }
}
