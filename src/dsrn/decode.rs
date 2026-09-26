//! Reading an encoding back.
//!
//! It walks the same tree the descent wrote, in the same order,
//! reading the same fields. Every field is fixed width once the code
//! and the region are known, so there is exactly one way to read any
//! stream and no place where the two halves could drift apart.
//!
//! It keeps the one thing the descent kept: which cells are taken.
//! That is what says how many payload bits a binding has -- a tile a
//! region below took whole has none -- and it is knowable here
//! because the children described again are read before the region
//! that described them writes what they left.

use crate::dsrn::cost::{below_the_grammar, tile_size_field_width};
use crate::dsrn::nesting::Knobs;
use crate::dsrn::nesting_data::{
    Encoded, RegionMask, A_SIZE_THAT_MEANS_COPY, BIND, CHILD_MASK_WIDTH, CODE_WIDTH, COPY,
    DIRECTION_WIDTH, FINEST_LEVEL_WITH_A_GRAMMAR, MASK, SUBDIVIDE,
};
use crate::dsrn::four_by_four::{
    BIND_OR_SKIP_WIDTH, COPY_OR_LEAVE_WIDTH, MASKED, MASKED_OR_NOT_WIDTH, TILES_OF_ONE,
    TILE_SIZE_WIDTH, THE_REST_COPY, BIND as BIND_AT_A_FOUR_BY_FOUR,
};
use crate::dsrn::region::{deepest_depth, Region, CHILD_COUNT, DIRECTIONS};
use crate::Bitmap;

/// Where the decoder has got to in the two streams, and what it has
/// put back so far.
struct Reading {
    tree: usize,
    payload: usize,
    taken: Bitmap,
}

impl Reading {
    fn new() -> Self {
        Self { tree: 0, payload: 0, taken: Bitmap::new() }
    }

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
///
/// The knobs have to be the ones the encode used. Most of them only
/// ever narrowed what the encoder would choose, and the stream says
/// which it chose; [`crate::dsrn::FourByFour`] is different, because
/// it changes what a 4x4 writes rather than what it picks.
pub fn decode(out: &Encoded, knobs: Knobs, bitmap: &mut Bitmap) {
    bitmap.reset();
    let mut reading = Reading::new();
    decode_region(&mut reading, out, knobs, bitmap, Region::whole_bitmap(), false);
}

/// Puts back one region: the children it described again, and then
/// whatever of itself they left.
fn decode_region(
    reading: &mut Reading,
    out: &Encoded,
    knobs: Knobs,
    bitmap: &mut Bitmap,
    region: Region,
    covered_from_above: bool,
) {
    if knobs.four_by_four.applies_to(region) {
        read_a_mask_over_the_children(reading, out, bitmap, region);
        return;
    }

    if knobs.four_by_four.is_its_own_grammar(region) {
        read_what_a_four_by_four_says(reading, out, bitmap, region, covered_from_above);
        return;
    }

    if below_the_grammar(region) {
        for cell in region.tiles_at_depth(deepest_depth(region.level)) {
            let value = reading.value(out);
            fill(&mut reading.taken, bitmap, cell, value);
        }
        take_region(reading, region);
        return;
    }

    let mut code = reading.take(out, CODE_WIDTH);
    let masked = code == MASK;
    if masked {
        code = reading.take(out, CODE_WIDTH);
    }
    // Unmasked, a binding and a copy describe no child again and a
    // subdivision describes all four.
    let mask = RegionMask(if masked {
        reading.take(out, CHILD_MASK_WIDTH)
    } else if code == SUBDIVIDE {
        RegionMask::EVERY.0
    } else {
        RegionMask::NONE.0
    });
    let depth = (code == BIND)
        .then(|| reading.take(out, tile_size_field_width(region.level)) as u64);
    // The one tile size a 4x4 has no size for says its children copy
    // themselves instead: a mask, then where each child it names
    // copies from. What it does not name is left where it is, for the
    // binding above to fill.
    if region.level == FINEST_LEVEL_WITH_A_GRAMMAR && depth == Some(A_SIZE_THAT_MEANS_COPY) {
        let mask = RegionMask(reading.take(out, CHILD_MASK_WIDTH));
        for (at, child) in region.children().into_iter().enumerate() {
            if !mask.describes(at) {
                continue;
            }
            let direction = reading.take(out, DIRECTION_WIDTH) as usize;
            let beside = child
                .neighbour(direction)
                .expect("a copy names a neighbour on the bitmap");
            copy_cells(&mut reading.taken, bitmap, child, beside);
            take_region(reading, child);
        }
        if !covered_from_above {
            for (at, child) in region.children().into_iter().enumerate() {
                if !mask.describes(at) {
                    take_region(reading, child);
                }
            }
        }
        return;
    }
    let depth = depth.map(|depth| depth as usize);
    let direction =
        (code == COPY).then(|| reading.take(out, DIRECTION_WIDTH) as usize);

    let covers_its_children = code != SUBDIVIDE;
    for (at, child) in region.children().into_iter().enumerate() {
        if mask.describes(at) {
            let covered = covered_from_above || covers_its_children;
            decode_region(reading, out, knobs, bitmap, child, covered);
        }
    }

    if let Some(depth) = depth {
        for tile in region.tiles_at_depth(depth) {
            if whole_region_taken(&reading.taken, tile) {
                continue;
            }
            let value = reading.value(out);
            fill(&mut reading.taken, bitmap, tile, value);
        }
    }
    if let Some(direction) = direction {
        let from = region.neighbour(direction).expect("a copy names a neighbour on the bitmap");
        copy_cells(&mut reading.taken, bitmap, region, from);
    }
    if code == SUBDIVIDE && !covered_from_above {
        // A child left out with no binding above it is clear, and
        // clear is something a copy may read.
        for (at, child) in region.children().into_iter().enumerate() {
            if !mask.describes(at) {
                take_region(reading, child);
            }
        }
    }
    let _ = DIRECTIONS;
}

/// Puts back a 4x4 in its own grammar.
fn read_what_a_four_by_four_says(
    reading: &mut Reading,
    out: &Encoded,
    bitmap: &mut Bitmap,
    region: Region,
    covered_from_above: bool,
) {
    let binds = reading.take(out, BIND_OR_SKIP_WIDTH) == BIND_AT_A_FOUR_BY_FOUR;
    let at_ones = binds && reading.take(out, TILE_SIZE_WIDTH) == TILES_OF_ONE;
    let masked = reading.take(out, MASKED_OR_NOT_WIDTH) == MASKED;
    // Binding at one tile a cell and taking all four would say what
    // skipping and taking all four says, so it says this instead.
    if at_ones && !masked {
        let direction = reading.take(out, DIRECTION_WIDTH) as usize;
        let from = region.neighbour(direction).expect("a copy names a neighbour on the bitmap");
        copy_cells(&mut reading.taken, bitmap, region, from);
        return;
    }
    let (mask, the_rest_copy) = if masked {
        let mask = RegionMask(reading.take(out, CHILD_MASK_WIDTH));
        (mask, reading.take(out, COPY_OR_LEAVE_WIDTH) == THE_REST_COPY)
    } else {
        (RegionMask::EVERY, false)
    };

    for (at, child) in region.children().into_iter().enumerate() {
        if !mask.describes(at) {
            if the_rest_copy {
                let direction = reading.take(out, DIRECTION_WIDTH) as usize;
                let beside = child
                    .neighbour(direction)
                    .expect("a copy names a neighbour on the bitmap");
                copy_cells(&mut reading.taken, bitmap, child, beside);
                take_region(reading, child);
            } else if !covered_from_above {
                take_region(reading, child);
            }
            continue;
        }
        if binds && !at_ones {
            let value = reading.value(out);
            fill(&mut reading.taken, bitmap, child, value);
            continue;
        }
        for cell in child.tiles_at_depth(deepest_depth(child.level)) {
            let value = reading.value(out);
            fill(&mut reading.taken, bitmap, cell, value);
        }
        take_region(reading, child);
    }
}

/// Puts back a 4x4 that always masks: the mask, then per child in
/// reading order a direction to copy from or its four cells.
fn read_a_mask_over_the_children(
    reading: &mut Reading,
    out: &Encoded,
    bitmap: &mut Bitmap,
    region: Region,
) {
    let mask = RegionMask(reading.take(out, CHILD_MASK_WIDTH));
    for (at, child) in region.children().into_iter().enumerate() {
        if mask.describes(at) {
            let direction = reading.take(out, DIRECTION_WIDTH) as usize;
            let beside = child
                .neighbour(direction)
                .expect("a copy names a neighbour on the bitmap");
            copy_cells(&mut reading.taken, bitmap, child, beside);
        } else {
            for cell in child.tiles_at_depth(deepest_depth(child.level)) {
                let value = reading.value(out);
                fill(&mut reading.taken, bitmap, cell, value);
            }
        }
        take_region(reading, child);
    }
    let _ = CHILD_COUNT;
}

/// Marks every cell of a region taken.
fn take_region(reading: &mut Reading, region: Region) {
    let (x, y) = region.top_left_cell();
    let side = region.side_in_cells();
    reading.taken.set_rect(x as i64, y as i64, (x + side - 1) as i64, (y + side - 1) as i64);
}

/// Whether every cell of a region is taken already.
fn whole_region_taken(taken: &Bitmap, region: Region) -> bool {
    let (x, y) = region.top_left_cell();
    let side = region.side_in_cells();
    for row in 0..side {
        for col in 0..side {
            if !taken.get((x + col) as u8, (y + row) as u8) {
                return false;
            }
        }
    }
    true
}

/// Writes one value over whatever of a tile is not taken, and takes
/// it.
fn fill(taken: &mut Bitmap, bitmap: &mut Bitmap, tile: Region, value: bool) {
    let (x, y) = tile.top_left_cell();
    let side = tile.side_in_cells();
    for row in 0..side {
        for col in 0..side {
            let (at_x, at_y) = ((x + col) as u8, (y + row) as u8);
            if taken.get(at_x, at_y) {
                continue;
            }
            taken.set(at_x, at_y);
            if value {
                bitmap.set(at_x, at_y);
            }
        }
    }
}

/// Copies one region's cells onto whatever of another's is not taken.
fn copy_cells(taken: &mut Bitmap, bitmap: &mut Bitmap, to: Region, from: Region) {
    let ((tx, ty), (fx, fy)) = (to.top_left_cell(), from.top_left_cell());
    let side = to.side_in_cells();
    for row in 0..side {
        for col in 0..side {
            let (at_x, at_y) = ((tx + col) as u8, (ty + row) as u8);
            if taken.get(at_x, at_y) {
                continue;
            }
            taken.set(at_x, at_y);
            if bitmap.get((fx + col) as u8, (fy + row) as u8) {
                bitmap.set(at_x, at_y);
            }
        }
    }
}
