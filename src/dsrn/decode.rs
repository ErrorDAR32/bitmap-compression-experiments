//! Reading an encoding back.
//!
//! It walks the same tree the descent wrote, in the same order,
//! reading the same fields. Every field is fixed width once the code
//! and the region are known, so there is exactly one way to read any
//! stream and no place where the two halves could drift apart.

use crate::dsrn::cost::{below_the_grammar, tile_size_field_width};
use crate::dsrn::nesting_data::{
    Encoded, RegionMask, BIND, CHILD_MASK_WIDTH, CODE_WIDTH, COPY, DIRECTION_WIDTH, MASK,
    SUBDIVIDE,
};
use crate::dsrn::region::{deepest_depth, Region, DIRECTIONS};
use crate::Bitmap;

/// Where the decoder has got to in the two streams.
#[derive(Default)]
pub struct Reading {
    tree: usize,
    payload: usize,
}

impl Reading {
    fn take(&mut self, out: &Encoded, width: usize) -> u64 {
        let got = out.tree.take(self.tree, width);
        self.tree += width;
        got
    }

    fn value(&mut self, out: &Encoded) -> bool {
        let got = out.payload.at(self.payload);
        self.payload += 1;
        got
    }
}

/// Reads the bitmap back.
pub fn decode(out: &Encoded, bitmap: &mut Bitmap) {
    bitmap.reset();
    let mut reading = Reading::default();
    decode_region(&mut reading, out, bitmap, Region::whole_bitmap());
}

/// Puts back one region, and whatever its description left out.
fn decode_region(reading: &mut Reading, out: &Encoded, bitmap: &mut Bitmap, region: Region) {
    if below_the_grammar(region) {
        for cell in region.tiles_at_depth(deepest_depth(region.level)) {
            let value = reading.value(out);
            fill(bitmap, cell, value);
        }
        return;
    }

    let mut code = reading.take(out, CODE_WIDTH);
    let masked = code == MASK;
    if masked {
        code = reading.take(out, CODE_WIDTH);
    }
    // Unmasked, a binding and a copy cover the whole region and a
    // subdivision covers none of it.
    let mask = RegionMask(if masked {
        reading.take(out, CHILD_MASK_WIDTH)
    } else if code == SUBDIVIDE {
        RegionMask::NONE.0
    } else {
        RegionMask::EVERY.0
    });

    match code {
        BIND => {
            let depth = reading.take(out, tile_size_field_width(region.level)) as usize;
            for tile in region.tiles_at_depth(depth) {
                if mask != RegionMask::EVERY && !mask.covers(region.child_holding(depth, tile)) {
                    continue;
                }
                let value = reading.value(out);
                fill(bitmap, tile, value);
            }
        }
        COPY => {
            let direction = reading.take(out, DIRECTION_WIDTH) as usize;
            let from = region
                .neighbour(direction)
                .expect("a copy names a neighbour on the bitmap");
            if mask == RegionMask::EVERY {
                copy_cells(bitmap, region, from);
            } else {
                let theirs = from.children();
                for (at, mine) in region.children().into_iter().enumerate() {
                    if mask.covers(at) {
                        copy_cells(bitmap, mine, theirs[at]);
                    }
                }
            }
        }
        // Subdividing says nothing, and a child it covers stays as the
        // decoder found it, which is clear.
        _ => {}
    }

    for (at, child) in region.children().into_iter().enumerate() {
        if !mask.covers(at) {
            decode_region(reading, out, bitmap, child);
        }
    }
    let _ = DIRECTIONS;
}

/// Writes a whole tile into the bitmap.
fn fill(bitmap: &mut Bitmap, tile: Region, value: bool) {
    if !value {
        return;
    }
    let (x, y) = tile.top_left_cell();
    let side = tile.side_in_cells();
    bitmap.set_rect(x as i64, y as i64, (x + side - 1) as i64, (y + side - 1) as i64);
}

/// Copies one region's cells onto another's, a cell at a time.
fn copy_cells(bitmap: &mut Bitmap, to: Region, from: Region) {
    let ((tx, ty), (fx, fy)) = (to.top_left_cell(), from.top_left_cell());
    let side = to.side_in_cells();
    for row in 0..side {
        for col in 0..side {
            if bitmap.get((fx + col) as u8, (fy + row) as u8) {
                bitmap.set((tx + col) as u8, (ty + row) as u8);
            }
        }
    }
}
