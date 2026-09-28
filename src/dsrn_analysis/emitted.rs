//! Patterns whose right answer is known, which dsrn's own tests
//! (`src/dsrn/nesting_tests.rs`) are checked on. The experiments on dsrn
//! that lived beside this are gone -- they are in git -- and this goes
//! with dsrn.

use crate::Bitmap;

/// A checkerboard of squares `side` cells across.
fn checkerboard(side: usize) -> Bitmap {
    let mut bitmap = Bitmap::new();
    for y in 0..256 {
        for x in 0..256 {
            if (x / side + y / side) % 2 == 0 {
                bitmap.set(x as u8, y as u8);
            }
        }
    }
    bitmap
}

/// Patterns whose right answer can be worked out by hand.
pub fn known_patterns() -> Vec<(String, Bitmap)> {
    let mut halves = Bitmap::new();
    halves.set_rect(0, 0, 127, 255);
    let mut one = Bitmap::new();
    one.set(128, 128);
    let mut full = Bitmap::new();
    full.set_rect(0, 0, 255, 255);
    vec![
        ("empty".to_string(), Bitmap::new()),
        ("every cell set".to_string(), full),
        ("one cell set".to_string(), one),
        ("halves".to_string(), halves),
        ("checkerboard of 1".to_string(), checkerboard(1)),
        ("checkerboard of 2".to_string(), checkerboard(2)),
        ("checkerboard of 8".to_string(), checkerboard(8)),
    ]
}
