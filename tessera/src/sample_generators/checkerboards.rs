//! Checkerboards: the one family drawn rather than grown, and a stress
//! test for all of it. Squares of an odd side never line up with the
//! power-of-two tiles the quadtree cuts along: tiles inside a square
//! are homogeneous, tiles across a cut subdivide, and the pattern
//! repeats at an offset no tile size matches exactly, so copies and
//! masking copies turn up everywhere. No seed: a checkerboard is
//! settled by its square side alone.

use bitmap::Bitmap;

/// The smallest square side measured: the smallest odd side above one
/// cell, which would be the plain alternating board.
pub const SMALLEST_SQUARE_SIDE: u8 = 3;

/// The largest square side measured: the largest odd side under 32,
/// so the board still holds eight squares across.
pub const LARGEST_SQUARE_SIDE: u8 = 31;

/// A checkerboard of `square_side` cell squares, the top left square
/// clear.
pub fn checkerboard(square_side: u8) -> Bitmap {
    let mut bitmap = Bitmap::new();
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if (x / square_side + y / square_side) % 2 == 1 {
                bitmap.set(x, y);
            }
        }
    }
    bitmap
}

/// Every odd square side measured, with its checkerboard.
pub fn checkerboards() -> impl Iterator<Item = (u8, Bitmap)> {
    (SMALLEST_SQUARE_SIDE..=LARGEST_SQUARE_SIDE).step_by(2).map(|side| (side, checkerboard(side)))
}
