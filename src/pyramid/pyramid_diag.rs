//! Reading a tile the slow way, so the fast way can be checked
//! against it.
//!
//! The pyramid folds each level from the one below, five bit
//! operations at a time over packed planes, and a fold that is wrong
//! is wrong quietly -- it reports a tile homogeneous that is not, and
//! the encoding built on it loses cells somewhere far away. So there
//! is a second answer here, got by reading every cell of the tile,
//! that the first is held to.

use super::pyramid_data::{tile_side, tiles_across, Pyramid, CELL_LEVEL, DIRECTIONS};
use crate::Bitmap;

/// What a tile holds, by reading every cell of it.
pub fn tile_by_reading_the_cells(bitmap: &Bitmap, level: usize, x: usize, y: usize) -> Option<bool> {
    let side = tile_side(level);
    let (x0, y0) = (x * side, y * side);
    let first = bitmap.get(x0 as u8, y0 as u8);
    for row in 0..side {
        for col in 0..side {
            if bitmap.get((x0 + col) as u8, (y0 + row) as u8) != first {
                return None;
            }
        }
    }
    Some(first)
}

/// Whether a tile holds the same cells as a neighbour reading order
/// puts before it, by reading every cell of both.
pub fn copyable_by_reading_the_cells(
    bitmap: &Bitmap,
    level: usize,
    x: usize,
    y: usize,
) -> bool {
    let (side, across) = (tile_side(level), tiles_across(level) as isize);
    DIRECTIONS.iter().any(|&(dx, dy)| {
        let (at_x, at_y) = (x as isize + dx, y as isize + dy);
        if at_x < 0 || at_y < 0 || at_x >= across {
            return false;
        }
        let (mine_x, mine_y) = (x * side, y * side);
        let (their_x, their_y) = (at_x as usize * side, at_y as usize * side);
        (0..side).all(|row| {
            (0..side).all(|col| {
                bitmap.get((mine_x + col) as u8, (mine_y + row) as u8)
                    == bitmap.get((their_x + col) as u8, (their_y + row) as u8)
            })
        })
    })
}

/// Whether a pyramid agrees with the cells, at every tile of every
/// level it holds.
pub fn agrees_with_the_cells(pyramid: &Pyramid, bitmap: &Bitmap) -> Result<(), String> {
    for level in 0..CELL_LEVEL {
        let across = tiles_across(level);
        for y in 0..across {
            for x in 0..across {
                let folded = pyramid.tile(level, x, y);
                let read = tile_by_reading_the_cells(bitmap, level, x, y);
                if folded != read {
                    return Err(format!(
                        "level {level}, tile ({x}, {y}) of {across} across: \
                         the pyramid says {folded:?} and the cells say {read:?}"
                    ));
                }
                let quick = pyramid.copyable(level, x, y);
                let slow = copyable_by_reading_the_cells(bitmap, level, x, y);
                if quick != slow {
                    return Err(format!(
                        "level {level}, tile ({x}, {y}) of {across} across: \
                         the pyramid says copyable {quick} and the cells say {slow}"
                    ));
                }
            }
        }
    }
    Ok(())
}
