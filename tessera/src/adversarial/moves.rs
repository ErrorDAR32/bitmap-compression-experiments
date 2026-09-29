//! The changes a search tries, each confined to the searched area.
//! Flipping single cells alone would wander; these aim at what the
//! encoders are built on -- tile boundaries, homogeneity, copies that
//! almost match, and structure no power-of-two tile lines up with.

use crate::rng::Rng;
use crate::pyramids::copyable::FINEST_COPY_LEVEL;
use crate::tile::{tile_side, Tile, CELL_LEVEL, DIRECTIONS};
use crate::Bitmap;

/// The largest square a painted rectangle or a checkerboard patch
/// spans, as a share of the area's side: big enough to cross several
/// tile boundaries, small enough to stay one local change.
const PATCH_SHARE_OF_SIDE: usize = 4;

/// The longest checkerboard period tried; odd periods only, so none
/// lines up with the grid...
const LONGEST_CHECKER_PERIOD: u64 = 15;
/// ...from this shortest one up.
const SHORTEST_CHECKER_PERIOD: u64 = 3;

/// Flips one cell: set to clear, clear to set.
pub fn flip(bitmap: &mut Bitmap, x: u8, y: u8) {
    if bitmap.get(x, y) {
        bitmap.unset(x, y);
    } else {
        bitmap.set(x, y);
    }
}

/// A cell of `area`, anywhere.
fn some_cell(rng: &mut Rng, area: Tile) -> (u8, u8) {
    let (left, top, right, bottom) = area.cell_rect();
    (rng.between(left as u64, right as u64) as u8, rng.between(top as u64, bottom as u64) as u8)
}

/// A tile inside `area`, at a level from `coarsest` to `finest`.
fn some_tile_inside(rng: &mut Rng, area: Tile, coarsest: u8, finest: u8) -> Tile {
    let level = rng.between(coarsest as u64, finest as u64) as u8;
    let (x, y) = some_cell(rng, area);
    Tile { level: CELL_LEVEL, x, y }.ancestor(level)
}

/// A kind of change.
pub type Change = fn(&mut Rng, &mut Bitmap, Tile);

/// Every kind of change, which a search picks among.
pub const CHANGES: [Change; 5] = [flip_a_cell, flip_a_tile, paint_a_rectangle, copy_almost, xor_a_checkerboard];

/// Flips one cell of `area`, anywhere.
fn flip_a_cell(rng: &mut Rng, bitmap: &mut Bitmap, area: Tile) {
    let (x, y) = some_cell(rng, area);
    flip(bitmap, x, y);
}

/// Every cell of one tile, which breaks or makes a homogeneous tile.
fn flip_a_tile(rng: &mut Rng, bitmap: &mut Bitmap, area: Tile) {
    let tile = some_tile_inside(rng, area, area.level + 1, CELL_LEVEL);
    let (left, top, right, bottom) = tile.cell_rect();
    for y in top..=bottom {
        for x in left..=right {
            flip(bitmap, x, y);
        }
    }
}

/// A rectangle set or cleared at any offset and size, so its edges
/// cut tiles.
fn paint_a_rectangle(rng: &mut Rng, bitmap: &mut Bitmap, area: Tile) {
    let longest = (area.side_in_cells() / PATCH_SHARE_OF_SIDE).max(1) as u64;
    let (x, y) = some_cell(rng, area);
    let (_, _, right, bottom) = area.cell_rect();
    let x1 = (x as u64 + rng.below(longest)).min(right as u64) as i64;
    let y1 = (y as u64 + rng.below(longest)).min(bottom as u64) as i64;
    if rng.below(2) == 0 {
        bitmap.set_rect(x as i64, y as i64, x1, y1);
    } else {
        bitmap.unset_rect(x as i64, y as i64, x1, y1);
    }
}

/// A tile made a copy of a neighbour it could copy from, then one cell
/// of it changed: a copy that almost fits, for the copies, the masking
/// copies and the complex tiles to argue over.
fn copy_almost(rng: &mut Rng, bitmap: &mut Bitmap, area: Tile) {
    if area.level + 1 > FINEST_COPY_LEVEL {
        return flip_a_cell(rng, bitmap, area);
    }
    let tile = some_tile_inside(rng, area, area.level + 1, FINEST_COPY_LEVEL);
    let direction = rng.below(DIRECTIONS.len() as u64) as u8;
    let Some(source) = tile.neighbour(direction) else { return };
    let side = tile_side(tile.level);
    let ((to_x, to_y), (from_x, from_y)) = (tile.top_left_cell(), source.top_left_cell());
    for dy in 0..side {
        for dx in 0..side {
            let (source_x, source_y) = ((from_x as usize + dx) as u8, (from_y as usize + dy) as u8);
            let (copy_x, copy_y) = ((to_x as usize + dx) as u8, (to_y as usize + dy) as u8);
            if bitmap.get(source_x, source_y) != bitmap.get(copy_x, copy_y) {
                flip(bitmap, copy_x, copy_y);
            }
        }
    }
    let (x, y) = some_cell(rng, tile);
    flip(bitmap, x, y);
}

/// A patch of odd-period checkerboard XORed in: structure no tile size
/// matches.
fn xor_a_checkerboard(rng: &mut Rng, bitmap: &mut Bitmap, area: Tile) {
    let longest = (area.side_in_cells() / PATCH_SHARE_OF_SIDE).max(1) as u64;
    let period = rng.between(SHORTEST_CHECKER_PERIOD / 2, LONGEST_CHECKER_PERIOD / 2) * 2 + 1;
    let (x0, y0) = some_cell(rng, area);
    let (_, _, right, bottom) = area.cell_rect();
    let x1 = (x0 as u64 + rng.below(longest)).min(right as u64) as u8;
    let y1 = (y0 as u64 + rng.below(longest)).min(bottom as u64) as u8;
    for y in y0..=y1 {
        for x in x0..=x1 {
            if (x as u64 / period + y as u64 / period) % 2 == 1 {
                flip(bitmap, x, y);
            }
        }
    }
}
