//! The questions about a region that need the bitmap to answer.
//!
//! All of them are asked a cell at a time. Nothing here is written for
//! speed -- a question the algorithm states as "do these two squares
//! hold the same cells" is asked that way, so that reading the code
//! and reading the statement give the same answer.

use super::region_data::Region;
use crate::pyramid::Pyramid;
use crate::Bitmap;

/// Whether two regions of the same size hold the same cells.
pub fn same_cells(bitmap: &Bitmap, a: Region, b: Region) -> bool {
    let ((ax, ay), (bx, by)) = (a.top_left_cell(), b.top_left_cell());
    let side = a.side_in_cells();
    for row in 0..side {
        for col in 0..side {
            if bitmap.get((ax + col) as u8, (ay + row) as u8)
                != bitmap.get((bx + col) as u8, (by + row) as u8)
            {
                return false;
            }
        }
    }
    true
}

/// Whether every cell of a region is clear, which is what a region
/// left alone by a subdivision stays.
pub fn all_cells_clear(pyramid: &Pyramid, bitmap: &Bitmap, region: Region) -> bool {
    crate::pyramid::tile_of_bitmap(pyramid, bitmap, region.level, region.x, region.y)
        == Some(false)
}

/// Whether every cell of a region has been encoded already, and so
/// will be there for the decoder to copy from.
pub fn whole_region_encoded(encoded_cells: &Bitmap, region: Region) -> bool {
    let (x, y) = region.top_left_cell();
    let side = region.side_in_cells();
    for row in 0..side {
        for col in 0..side {
            if !encoded_cells.get((x + col) as u8, (y + row) as u8) {
                return false;
            }
        }
    }
    true
}
