//! What has to be true of the tile-or-subdivide tree: it covers every
//! cell exactly once, and rebuilding a bitmap from its leaves gives
//! back the bitmap it was built from.

#![cfg(test)]

use super::tile_or_subdivide::decide;
use crate::dsrn::region::Region;
use crate::pyramid::Pyramid;
use crate::samples;
use crate::Bitmap;

#[test]
fn covers_every_cell_once_and_matches_the_bitmap() {
    let mut pyramid = Pyramid::new();
    for (family, maps) in samples::every_family() {
        for (case, bitmap) in maps.iter().enumerate() {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            let (mut leaves, mut subdivisions) = (Vec::new(), 0usize);
            decide(&pyramid, bitmap, Region::whole_bitmap(), &mut leaves, &mut subdivisions);

            let area: usize = leaves
                .iter()
                .map(|leaf| { let side = leaf.region.side_in_cells(); side * side })
                .sum();
            assert_eq!(area, 256 * 256, "{family}, case {case}: left cells uncovered or double-covered");

            let mut rebuilt = Bitmap::new();
            for leaf in &leaves {
                if leaf.value {
                    let (x, y) = leaf.region.top_left_cell();
                    let side = leaf.region.side_in_cells();
                    rebuilt.set_rect(x as i64, y as i64, (x + side - 1) as i64, (y + side - 1) as i64);
                }
            }
            for y in 0..=u8::MAX {
                for x in 0..=u8::MAX {
                    assert_eq!(bitmap.get(x, y), rebuilt.get(x, y), "{family}, case {case}: differs at ({x}, {y})");
                }
            }
        }
    }
}
