//! Pairing the greedy pass's precomputed facts with an actual
//! quadtree: a real, round-tripping codec.
//!
//! At every region: try a whole-region copy first, since it is
//! cheapest, chosen purely on whether its cells agree with a
//! candidate's -- never on whether that candidate has been decided
//! yet, since the bitmap it is read from does not change no matter
//! what order anything gets described in. Failing that, look at the
//! region's own four children -- the one fixed size this scheme ever
//! tiles at, since the pyramid already answers "is this child
//! homogeneous" in one lookup. If every child is homogeneous, bind
//! all four and stop. If describing the homogeneous ones costs less
//! than leaving the rest as one bit a cell, bind the ones that are and
//! subdivide into the ones that are not. Otherwise nothing here is
//! worth saying, and the tree simply does not go any further: no bits
//! are spent recording that, the region's cells stay uncovered, and a
//! trailing pass fills every cell the tree never touched with its own
//! raw bit, at the very end of the stream. A region one level above
//! cells never even tries to tile -- its children are single cells,
//! always "homogeneous" trivially, so tiling them would always
//! "succeed" while never once compressing anything; leaving them as
//! holes for the trailing pass is strictly cheaper.
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
//! 0: nothing here -- one region above cells, this is a hole, stop
//!    (any other level) recurse into all four children, unconditionally
//! ```
//!
//! (trailing, once the tree is done) one bit for every cell the tree
//! never covered, in reading order
//!
//! The mask has to come before the tiling's value bits: a decoder
//! cannot know how many bits to read, or for which children, before
//! it knows which children the mask is deferring. A plain "recurse
//! into all four" needs no mask at all -- there is nothing here to
//! defer, so every child gets visited.
//!
//! Since a copy's source is chosen on content alone, it may not be
//! resolved yet by the time the tree reaches it, and it may even be a
//! hole the trailing pass has not read yet. Decoding is therefore two
//! passes, exactly like [`crate::dsrn_exp::tile_stream`]: first the
//! tree's own structure and the trailing raw bits are read in one
//! linear sweep, recording what every cell is -- a value, or a
//! direction to read one from -- without resolving any of it yet;
//! then values are resolved by repeated sweeps, a copy deferring to
//! the next sweep whenever its source is not resolved yet. No cycle is
//! possible: a copy always names something reading order puts before
//! it, so this always finishes, backed by an assertion rather than
//! blind trust.

use crate::dsrn::region::{same_cells, Region, DIRECTIONS};
use crate::dsrn::stream::EncodedBitmap;
use crate::dsrn_exp::greedy_tiles::FarCopyable;
use crate::pyramid::{tile_of_bitmap, Pyramid, CELL_LEVEL};
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

/// Which direction a whole region copies from, if any, and whether
/// that is a near neighbour of the region itself or a far one of its
/// parent -- decided purely on whether the cells agree, the same
/// static fact regardless of what has been described so far.
///
/// The pyramid's own `copyable` bit, and the greedy tiler's matching
/// `FarCopyable` cache, are precomputed once a bitmap and answered in
/// O(1); without them, every region that fails both would still pay
/// for up to four full `same_cells` scans it was never going to use.
fn copy_choice(pyramid: &Pyramid, far_copyable: &FarCopyable, bitmap: &Bitmap, region: Region) -> Option<(bool, usize)> {
    if pyramid.copyable(region.level, region.x, region.y) {
        let near = (0..DIRECTIONS.len()).find(|&direction| {
            region.neighbour(direction).is_some_and(|beside| same_cells(bitmap, region, beside))
        });
        if let Some(direction) = near {
            return Some((false, direction));
        }
    }
    if !far_copyable.get(region) {
        return None;
    }
    let parent = Region { level: region.level - 1, x: region.x / 2, y: region.y / 2 };
    let (child_dx, child_dy) = (region.x % 2, region.y % 2);
    (0..DIRECTIONS.len()).find_map(|direction| {
        let beside_parent = parent.neighbour(direction)?;
        let far =
            Region { level: region.level, x: beside_parent.x * 2 + child_dx, y: beside_parent.y * 2 + child_dy };
        same_cells(bitmap, region, far).then_some((true, direction))
    })
}

/// Encodes a bitmap top-down from the whole bitmap inward, then fills
/// every cell the tree left uncovered with its own raw bit.
pub fn encode(pyramid: &Pyramid, bitmap: &Bitmap) -> EncodedBitmap {
    let mut out = EncodedBitmap::default();
    let far_copyable = FarCopyable::build(bitmap);
    let mut covered = Bitmap::new();
    encode_region(pyramid, &far_copyable, bitmap, Region::whole_bitmap(), &mut covered, &mut out);

    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if !covered.get(x, y) {
                out.push_value(bitmap.get(x, y) as u64, 1);
            }
        }
    }
    out
}

fn encode_region(
    pyramid: &Pyramid,
    far_copyable: &FarCopyable,
    bitmap: &Bitmap,
    region: Region,
    covered: &mut Bitmap,
    out: &mut EncodedBitmap,
) {
    if let Some((far, direction)) = copy_choice(pyramid, far_copyable, bitmap, region) {
        out.push_value(1, BIND_WIDTH);
        out.push_value(COPY, CODE_WIDTH);
        out.push_value(far as u64, FAR_WIDTH);
        out.push_value(direction as u64, DIRECTION_WIDTH);
        out.push_value(0, SUBDIVIDE_WIDTH);
        mark_covered(covered, region);
        return;
    }

    if region.level == CELL_LEVEL - 1 {
        // Its children are single cells, always homogeneous trivially:
        // tiling them would always "succeed" while never compressing
        // anything. Leave them as holes for the trailing raw pass.
        out.push_value(0, BIND_WIDTH);
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
                mark_covered(covered, children[i]);
            }
        }
        if subdivide {
            for (i, value) in values.iter().enumerate() {
                if value.is_none() {
                    encode_region(pyramid, far_copyable, bitmap, children[i], covered, out);
                }
            }
        }
        return;
    }

    // Nothing here is worth saying. No mask, no bits beyond the bind
    // bit itself: every child gets visited, since there is nothing to
    // defer.
    out.push_value(0, BIND_WIDTH);
    for child in children {
        encode_region(pyramid, far_copyable, bitmap, child, covered, out);
    }
}

/// Marks every cell of a region as covered by the tree.
fn mark_covered(covered: &mut Bitmap, region: Region) {
    let (x, y) = region.top_left_cell();
    let side = region.side_in_cells();
    covered.set_rect(x as i64, y as i64, (x + side - 1) as i64, (y + side - 1) as i64);
}

/// What a cell's owner says, once the tree and the trailing raw pass
/// have both been read: its own value, or a direction and distance to
/// read the matching cell from.
#[derive(Clone, Copy)]
enum Owner {
    Bound(bool),
    Copied { direction: usize, side: usize, far: bool },
}

/// Decodes a stream written by [`encode`].
pub fn decode(stream: &EncodedBitmap) -> Bitmap {
    let mut owner: Vec<Option<Owner>> = vec![None; 256 * 256];
    let mut covered = Bitmap::new();
    let mut at = 0usize;
    decode_region(stream, &mut at, Region::whole_bitmap(), &mut covered, &mut owner);

    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if !covered.get(x, y) {
                let value = stream.take(at, 1) != 0;
                at += 1;
                owner[y as usize * 256 + x as usize] = Some(Owner::Bound(value));
            }
        }
    }

    resolve(&owner)
}

fn decode_region(stream: &EncodedBitmap, at: &mut usize, region: Region, covered: &mut Bitmap, owner: &mut [Option<Owner>]) {
    let bind = stream.take(*at, BIND_WIDTH) != 0;
    *at += BIND_WIDTH;

    if !bind {
        if region.level == CELL_LEVEL - 1 {
            return;
        }
        for child in region.children() {
            decode_region(stream, at, child, covered, owner);
        }
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
        mark_owner(owner, region, Owner::Copied { direction, side: region.side_in_cells(), far });
        mark_covered(covered, region);
        if subdivide {
            let mask = stream.take(*at, MASK_WIDTH);
            *at += MASK_WIDTH;
            let children = region.children();
            for i in 0..4 {
                if mask & (1 << i) != 0 {
                    decode_region(stream, at, children[i], covered, owner);
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
            mark_owner(owner, children[i], Owner::Bound(value));
            mark_covered(covered, children[i]);
        }
    }
    if subdivide {
        for i in 0..4 {
            if mask & (1 << i) != 0 {
                decode_region(stream, at, children[i], covered, owner);
            }
        }
    }
}

/// Records every cell of a region as sharing one owner.
fn mark_owner(owner: &mut [Option<Owner>], region: Region, says: Owner) {
    let (x, y) = region.top_left_cell();
    let side = region.side_in_cells();
    for row in 0..side {
        for col in 0..side {
            owner[(y + row) * 256 + (x + col)] = Some(says);
        }
    }
}

/// Resolves every cell's owner into an actual bitmap, deferring a copy
/// to the next sweep whenever its source is not resolved yet.
fn resolve(owner: &[Option<Owner>]) -> Bitmap {
    let mut bitmap = Bitmap::new();
    let mut resolved = Bitmap::new();
    let mut left = 256 * 256;
    while left > 0 {
        let before = left;
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                if resolved.get(x, y) {
                    continue;
                }
                let value = match owner[y as usize * 256 + x as usize].expect("every cell got an owner") {
                    Owner::Bound(value) => Some(value),
                    Owner::Copied { direction, side, far } => {
                        let step = side * if far { 2 } else { 1 };
                        let (dx, dy) = DIRECTIONS[direction];
                        let fx = (x as isize + dx * step as isize) as u8;
                        let fy = (y as isize + dy * step as isize) as u8;
                        resolved.get(fx, fy).then(|| bitmap.get(fx, fy))
                    }
                };
                let Some(value) = value else { continue };
                if value {
                    bitmap.set(x, y);
                }
                resolved.set(x, y);
                left -= 1;
            }
        }
        assert!(left < before, "nothing resolved in a whole pass: a cycle exists that should be impossible");
    }
    bitmap
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
