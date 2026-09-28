//! The greedy tiler, the first pass: one rule, asked of every tile size
//! from the whole bitmap down to single cells, coarsest first, skipping
//! anything a coarser tile already claimed -- homogeneous? bind it.
//! Else copyable (a same-size neighbour, or, one level up, a same-size
//! neighbour of the tile's own parent)? copy it. Else leave it for its
//! four children to try for themselves. No comparison ever happens
//! between sizes; a tile that qualifies is taken immediately. Cells are
//! always homogeneous, so the pass always covers the whole bitmap.

use crate::gct::pyramids::copyable::{Copyable, FAR_DISTANCE};
use crate::gct::pyramids::homogeneity::Homogeneity;
use crate::gct::pyramids::placements::{Placement, Placements};
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{same_cells, tiles_across, Tile, CELL_LEVEL, DIRECTIONS};
use crate::Bitmap;

/// Places tiles over one bitmap, biggest first; what it placed is a
/// [placements pyramid](crate::gct::pyramids::placements). Reads the
/// bitmap's own homogeneity and copyable pyramids, and the bitmap itself
/// only to find which direction a copyable tile copies from.
pub fn greedy_tiler(bitmap: &Bitmap, homogeneity: &Pyramid, copyable: &Pyramid) -> Pyramid {
    let mut claimed = Bitmap::new();
    let mut placements = Pyramid::placements();

    for level in 0..=CELL_LEVEL {
        let across = tiles_across(level);
        for y in 0..across {
            for x in 0..across {
                let tile = Tile { level, x, y };
                // Tiles are placed biggest first and never overlap, so a
                // claimed corner means a coarser tile covers all of this
                // one -- same-size tiles partition the plane, and nothing
                // smaller has run yet.
                if tile.top_left_value(&claimed) {
                    continue;
                }
                let placement = if let Some(value) = homogeneity.homogeneous_value(tile) {
                    Placement::Bound(value)
                } else if let Some((far, direction)) = copy_choice(copyable, bitmap, tile) {
                    Placement::Copied { far, direction }
                } else {
                    continue;
                };
                tile.set_in(&mut claimed);
                placements.place(tile, placement);
            }
        }
    }
    placements
}

/// Which direction a tile copies from, if any, and whether that is a
/// far copy (a same-size neighbour of the tile's parent, at the tile's
/// own child position within it) rather than a near one (a same-size
/// neighbour of the tile itself).
fn copy_choice(copyable: &Pyramid, bitmap: &Bitmap, tile: Tile) -> Option<(bool, usize)> {
    if copyable.near_copyable(tile) {
        let near = (0..DIRECTIONS.len())
            .find(|&direction| tile.neighbour(direction).is_some_and(|beside| same_cells(bitmap, tile, beside)));
        if let Some(direction) = near {
            return Some((false, direction));
        }
    }
    if !copyable.far_copyable(tile) {
        return None;
    }
    (0..DIRECTIONS.len()).find_map(|direction| {
        let far = tile.neighbour_at(direction, FAR_DISTANCE)?;
        same_cells(bitmap, tile, far).then_some((true, direction))
    })
}
