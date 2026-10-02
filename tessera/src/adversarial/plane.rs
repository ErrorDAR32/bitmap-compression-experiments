//! From one adversarial window to the whole plane. Both encoders are
//! quadtrees, so a bad window stays bad wherever it sits -- but sixteen
//! copies of one window would just be copied. So each of the sixteen
//! window positions gets its own one of the window's sixteen variants:
//! four rotations, each mirrored or not, each inverted or not. Copies
//! only ever translate, so none of the variants is a copy of another
//! unless the window is itself symmetric.

use crate::tile::Tile;
use bitmap::Bitmap;

/// A window can be turned four ways...
const ROTATIONS: usize = 4;
/// ...mirrored or not...
const MIRRORINGS: usize = 2;
/// ...and inverted or not...
const INVERSIONS: usize = 2;
/// ...so this many variants of it fill the plane.
const VARIANTS: usize = ROTATIONS * MIRRORINGS * INVERSIONS;

/// Where `(x, y)` of a `side` square lands under a variant.
fn transformed(variant: usize, side: usize, x: usize, y: usize) -> (usize, usize) {
    let last = side - 1;
    let (x, y) = if (variant / ROTATIONS) % MIRRORINGS == 1 { (last - x, y) } else { (x, y) };
    match variant % ROTATIONS {
        0 => (x, y),
        1 => (last - y, x),
        2 => (last - x, last - y),
        _ => (y, last - x),
    }
}

/// The plane filled with `window`'s variants, `window` being what
/// `bitmap` holds at `area`, one variant per tile of `area`'s size.
pub fn fill_the_plane(bitmap: &Bitmap, area: Tile) -> Bitmap {
    let side = area.side_in_cells();
    let (left, top) = area.top_left_cell();
    let mut plane = Bitmap::new();
    for (variant, spot) in Tile::all_of_level(area.level).enumerate() {
        let variant = variant % VARIANTS;
        let invert = variant / (ROTATIONS * MIRRORINGS) == 1;
        let (to_x, to_y) = spot.top_left_cell();
        for y in 0..side {
            for x in 0..side {
                let value = bitmap.get(left + x as u8, top + y as u8) != invert;
                let (transformed_x, transformed_y) = transformed(variant, side, x, y);
                if value {
                    plane.set(to_x + transformed_x as u8, to_y + transformed_y as u8);
                }
            }
        }
    }
    plane
}
