//! Pairing the greedy pass's precomputed facts with an actual
//! quadtree: a real, round-tripping codec, not just a decision count.
//!
//! At every region: try a whole-region copy first, since it is
//! cheapest. Failing that, look at the region's own four children --
//! that is the one fixed size this scheme ever tiles at, since the
//! pyramid already answers "is this child homogeneous" in one lookup,
//! no scanning needed. If every child is homogeneous, bind all four
//! and stop, no subdivide bit spent. If describing the homogeneous
//! ones costs less than leaving the rest as one bit a cell, bind the
//! ones that are and subdivide into the ones that are not. Otherwise
//! nothing here is worth saying at all, and it just subdivides into
//! all four, plainly.
//!
//! # The grammar
//!
//! ```text
//! 1: bind
//!   0: copy   + 1 far/near bit + 2 direction bits
//!   1: tiling + 3 size bits (always this region's own children)
//!   + 1 subdivide bit
//!     (if set) + 4 bit mask
//!   (if tiling) 1 value bit for every child the mask does not name
//!   (if the subdivide bit was set) recurse into every child the mask names
//! 0: skip -- always subdivides
//!   + 4 bit mask, recurse into every child the mask names
//! ```
//!
//! The mask has to come before the tiling's value bits: a decoder
//! cannot know how many bits to read, or for which children, before
//! it knows which children the mask is deferring.
//!
//! A copy is only offered when its source is already fully resolved
//! in the same top-down order the decoder will walk -- not merely
//! content-eligible -- for exactly the reason dsrn's own production
//! encoder keeps its `region_taken` check: a single-pass decoder that
//! never defers has to have its sources ready the moment it reads
//! them.

use crate::dsrn::region::{same_cells, Region, DIRECTIONS};
use crate::dsrn::stream::EncodedBitmap;
use crate::pyramid::{tile_of_bitmap, tiles_across, tiles_in_level, Pyramid, CELL_LEVEL};
use crate::Bitmap;

const BIND_WIDTH: usize = 1;
const CODE_WIDTH: usize = 1;
const FAR_WIDTH: usize = 1;
const DIRECTION_WIDTH: usize = 2;
const SIZE_WIDTH: usize = 3;
const SUBDIVIDE_WIDTH: usize = 1;
const MASK_WIDTH: usize = 4;

const COPY: u64 = 0;
const TILING: u64 = 1;

/// A bit per region, per level, for whether it is already fully
/// resolved in the traversal order both sides walk -- the same
/// upward-folding tracker dsrn's own `region_taken` is, kept local
/// here since this is a different codec, not dsrn itself.
struct Taken {
    marked: Vec<Vec<bool>>,
}

impl Taken {
    fn new() -> Self {
        Self { marked: (0..=CELL_LEVEL).map(|level| vec![false; tiles_in_level(level)]).collect() }
    }

    fn at(region: Region) -> usize {
        region.y * tiles_across(region.level) + region.x
    }

    fn whole_region_taken(&self, region: Region) -> bool {
        let mut here = region;
        loop {
            if self.marked[here.level][Self::at(here)] {
                return true;
            }
            if here.level == 0 {
                return false;
            }
            here = Region { level: here.level - 1, x: here.x / 2, y: here.y / 2 };
        }
    }

    fn mark(&mut self, region: Region) {
        self.marked[region.level][Self::at(region)] = true;
        let mut here = region;
        while here.level > 0 {
            let parent = Region { level: here.level - 1, x: here.x / 2, y: here.y / 2 };
            if !parent.children().into_iter().all(|c| self.whole_region_taken(c)) {
                break;
            }
            self.marked[parent.level][Self::at(parent)] = true;
            here = parent;
        }
    }
}

/// Which direction a whole region copies from, if any, and whether
/// that is a near neighbour of the region itself or a far one of its
/// parent -- only offered once the source is already taken.
fn copy_choice(pyramid: &Pyramid, bitmap: &Bitmap, taken: &Taken, region: Region) -> Option<(bool, usize)> {
    if pyramid.copyable(region.level, region.x, region.y) {
        let near = (0..DIRECTIONS.len()).find(|&direction| {
            region.neighbour(direction).is_some_and(|beside| {
                taken.whole_region_taken(beside) && same_cells(bitmap, region, beside)
            })
        });
        if let Some(direction) = near {
            return Some((false, direction));
        }
    }
    if region.level == 0 {
        return None;
    }
    let parent = Region { level: region.level - 1, x: region.x / 2, y: region.y / 2 };
    let (child_dx, child_dy) = (region.x % 2, region.y % 2);
    (0..DIRECTIONS.len()).find_map(|direction| {
        let beside_parent = parent.neighbour(direction)?;
        let far =
            Region { level: region.level, x: beside_parent.x * 2 + child_dx, y: beside_parent.y * 2 + child_dy };
        (taken.whole_region_taken(far) && same_cells(bitmap, region, far)).then_some((true, direction))
    })
}

/// Encodes a bitmap top-down from the whole bitmap inward.
pub fn encode(pyramid: &Pyramid, bitmap: &Bitmap) -> EncodedBitmap {
    let mut out = EncodedBitmap::default();
    let mut taken = Taken::new();
    encode_region(pyramid, bitmap, Region::whole_bitmap(), &mut taken, &mut out);
    out
}

fn encode_region(pyramid: &Pyramid, bitmap: &Bitmap, region: Region, taken: &mut Taken, out: &mut EncodedBitmap) {
    if let Some((far, direction)) = copy_choice(pyramid, bitmap, taken, region) {
        out.push_value(1, BIND_WIDTH);
        out.push_value(COPY, CODE_WIDTH);
        out.push_value(far as u64, FAR_WIDTH);
        out.push_value(direction as u64, DIRECTION_WIDTH);
        out.push_value(0, SUBDIVIDE_WIDTH);
        taken.mark(region);
        return;
    }

    let children = region.children();
    let values: [Option<bool>; 4] =
        std::array::from_fn(|i| tile_of_bitmap(pyramid, bitmap, children[i].level, children[i].x, children[i].y));
    let tiled_count = values.iter().filter(|v| v.is_some()).count();
    let child_side = children[0].side_in_cells();
    let leftover_cells = (4 - tiled_count) * child_side * child_side;

    if tiled_count == 4 || tiled_count < leftover_cells {
        out.push_value(1, BIND_WIDTH);
        out.push_value(TILING, CODE_WIDTH);
        out.push_value(region.level as u64, SIZE_WIDTH);
        let subdivide = tiled_count < 4;
        out.push_value(subdivide as u64, SUBDIVIDE_WIDTH);
        if subdivide {
            let mask: u64 = (0..4).filter(|&i| values[i].is_none()).map(|i| 1 << i).sum();
            out.push_value(mask, MASK_WIDTH);
        }
        for (i, value) in values.iter().enumerate() {
            if let Some(value) = value {
                out.push_value(*value as u64, 1);
                taken.mark(children[i]);
            }
        }
        if subdivide {
            for (i, value) in values.iter().enumerate() {
                if value.is_none() {
                    encode_region(pyramid, bitmap, children[i], taken, out);
                }
            }
        }
        taken.mark(region);
        return;
    }

    out.push_value(0, BIND_WIDTH);
    out.push_value(0b1111, MASK_WIDTH);
    for child in children {
        encode_region(pyramid, bitmap, child, taken, out);
    }
    taken.mark(region);
}

/// Decodes a stream written by [`encode`], one pass, top-down.
pub fn decode(stream: &EncodedBitmap) -> Bitmap {
    let mut bitmap = Bitmap::new();
    let mut taken = Taken::new();
    let mut at = 0usize;
    decode_region(stream, &mut at, Region::whole_bitmap(), &mut taken, &mut bitmap);
    bitmap
}

fn decode_region(stream: &EncodedBitmap, at: &mut usize, region: Region, taken: &mut Taken, bitmap: &mut Bitmap) {
    let bind = stream.take(*at, BIND_WIDTH) != 0;
    *at += BIND_WIDTH;

    if !bind {
        let mask = stream.take(*at, MASK_WIDTH);
        *at += MASK_WIDTH;
        let children = region.children();
        for i in 0..4 {
            if mask & (1 << i) != 0 {
                decode_region(stream, at, children[i], taken, bitmap);
            }
        }
        taken.mark(region);
        return;
    }

    let code = stream.take(*at, CODE_WIDTH);
    *at += CODE_WIDTH;

    if code == COPY {
        let far = stream.take(*at, FAR_WIDTH) != 0;
        *at += FAR_WIDTH;
        let direction = stream.take(*at, DIRECTION_WIDTH) as usize;
        *at += DIRECTION_WIDTH;
        let subdivide = stream.take(*at, SUBDIVIDE_WIDTH) != 0;
        *at += SUBDIVIDE_WIDTH;
        resolve_copy(bitmap, region, far, direction);
        taken.mark(region);
        if subdivide {
            let mask = stream.take(*at, MASK_WIDTH);
            *at += MASK_WIDTH;
            let children = region.children();
            for i in 0..4 {
                if mask & (1 << i) != 0 {
                    decode_region(stream, at, children[i], taken, bitmap);
                }
            }
        }
        return;
    }

    let child_level = stream.take(*at, SIZE_WIDTH) as usize + 1;
    *at += SIZE_WIDTH;
    debug_assert_eq!(child_level, region.level + 1, "tiling always names this region's own children");
    let subdivide = stream.take(*at, SUBDIVIDE_WIDTH) != 0;
    *at += SUBDIVIDE_WIDTH;
    let mask = if subdivide {
        let mask = stream.take(*at, MASK_WIDTH);
        *at += MASK_WIDTH;
        mask
    } else {
        0
    };

    let children = region.children();
    for i in 0..4 {
        if mask & (1 << i) == 0 {
            let value = stream.take(*at, 1) != 0;
            *at += 1;
            bind_region(bitmap, children[i], value);
            taken.mark(children[i]);
        }
    }
    if subdivide {
        for i in 0..4 {
            if mask & (1 << i) != 0 {
                decode_region(stream, at, children[i], taken, bitmap);
            }
        }
    }
    taken.mark(region);
}

/// Binds every cell of a region to a value.
fn bind_region(bitmap: &mut Bitmap, region: Region, value: bool) {
    if value {
        let (x, y) = region.top_left_cell();
        let side = region.side_in_cells();
        bitmap.set_rect(x as i64, y as i64, (x + side - 1) as i64, (y + side - 1) as i64);
    }
}

/// Copies a region's cells from its already-resolved source.
fn resolve_copy(bitmap: &mut Bitmap, region: Region, far: bool, direction: usize) {
    let source = if far {
        let parent = Region { level: region.level - 1, x: region.x / 2, y: region.y / 2 };
        let beside_parent = parent.neighbour(direction).expect("encoder only emits a far copy that exists");
        Region {
            level: region.level,
            x: beside_parent.x * 2 + region.x % 2,
            y: beside_parent.y * 2 + region.y % 2,
        }
    } else {
        region.neighbour(direction).expect("encoder only emits a near copy that exists")
    };
    let (rx, ry) = region.top_left_cell();
    let (sx, sy) = source.top_left_cell();
    let side = region.side_in_cells();
    for row in 0..side {
        for col in 0..side {
            if bitmap.get((sx + col) as u8, (sy + row) as u8) {
                bitmap.set((rx + col) as u8, (ry + row) as u8);
            }
        }
    }
}

/// Bits against dsrn's own, in the same currency [`crate::dsrn_exp::tile_stream`]
/// already compares in.
pub fn run() {
    use crate::dsrn::{encode as dsrn_encode, FourByFour, Knobs, Masking, Workspace};
    use crate::samples;

    let knobs = Knobs { masking: Masking::Anywhere, four_by_four: FourByFour::ItsOwnGrammar };
    let (mut pyramid, mut work, mut dsrn_out) =
        (Pyramid::new(), Workspace::new(), crate::dsrn::Encoded::default());

    for (family, maps) in samples::every_family() {
        let (mut dsrn_bits, mut our_bits) = (0usize, 0usize);
        for bitmap in &maps {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            dsrn_encode(&pyramid, bitmap, knobs, &mut work, &mut dsrn_out);
            dsrn_bits += dsrn_out.bits();
            our_bits += encode(&pyramid, bitmap).len();
        }
        let n = maps.len();
        println!(
            "\n  {family}, {n} bitmaps: dsrn {} bits a bitmap, tile-or-subdivide {} ({:+.1}%)",
            dsrn_bits / n,
            our_bits / n,
            100.0 * (our_bits as f64 - dsrn_bits as f64) / dsrn_bits as f64
        );
    }
}
