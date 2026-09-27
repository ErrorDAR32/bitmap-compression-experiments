//! Pairing the greedy pass's already-perfect tiling with a quadtree
//! that just has to say where each tile is.
//!
//! The tiles themselves are never decided here -- [`decide_tiles`]
//! already found the biggest-first, best set of homogeneous and
//! copyable tiles once, for the whole bitmap, and that set is treated
//! as fixed. All this file adds is a cheap way to say *where* each one
//! is: a plain quadtree, subdivided only where it has to be to reach a
//! tile that isn't the whole of the region it would otherwise bind.
//! Reaching a region that is itself exactly one placed tile costs one
//! bit to say so, whatever that tile's size; a region that isn't one
//! whole tile costs one bit to say it subdivides. Nothing here searches
//! for a size or compares a payload against leftover cells -- the
//! answer for every region is already known from the tile set, so
//! building the tree is a lookup, not a decision. The one exception is
//! how far a subdivide jumps, and even that is decided the same way:
//! if none of a region's four children is itself a placed tile either,
//! all four are only going to say "subdivide" too, so one shared bit
//! says that once and jumps straight to the sixteen grandchildren,
//! instead of four separate children each spending their own bit to
//! say the same thing apart.
//!
//! # The grammar
//!
//! ```text
//! (at any level down to one above cells)
//! 1: leaf -- this region is exactly one placed tile
//!    0: copy   + 1 far/near bit + 2 direction bits
//!    1: bind   + 1 value bit
//! 0: subdivide
//!    (more than two levels above cells) + 1 jump bit
//!      1: none of the four children is a placed tile either --
//!         go straight to the sixteen grandchildren
//!      0: recurse into the four children as usual
//!    (exactly two levels above cells) recurse into the four children;
//!    no jump bit, since their own children are one above cells, where
//!    this tree stops tracking depth entirely
//!
//! (one level above cells, in place of the above)
//! 1: this 2x2 is a homogeneous placed tile + 1 value bit
//! 0: it is not -- its four cells are holes, no further bits
//! ```
//!
//! The jump is never a comparison either: whenever it is checkable at
//! all (whenever none of the four children is a placed tile), taking
//! it is strictly cheaper than not, since it replaces four children's
//! worth of "subdivide" bits with the one bit that already had to be
//! spent to ask the question. There is no case where checking and
//! declining is better than checking and taking it.
//!
//! One level above cells never offers copy, and never subdivides
//! further, since cells are not tracked by this tree at all: a 2x2
//! that is only copyable, or that decide_tiles only managed to cover
//! with 1x1 tiles, is left entirely to the trailing raw pass, at one
//! bit a cell -- cheaper than a copy's own header for one tile that
//! small, and there is nothing finer here to subdivide into.
//!
//! Whatever the tree never covers -- 1x1 tiles and skipped 2x2s alike
//! -- gets exactly one raw bit a cell, in reading order, once the tree
//! is done. Decoding is two passes, exactly like
//! [`crate::dsrn_exp::tile_stream`]: a copy is chosen on content alone
//! by [`decide_tiles`], so its source may not be resolved yet by the
//! time the tree reaches it, and may even be a hole the trailing pass
//! has not read yet. The tree and the trailing bits are read first,
//! recording every cell as a value or a direction to read one from;
//! then values are resolved by repeated sweeps, deferring a copy to
//! the next one whenever its source is not resolved yet. No cycle is
//! possible -- a copy always names something reading order puts before
//! it -- so this always finishes, backed by an assertion rather than
//! blind trust.

use crate::dsrn::region::{Region, DIRECTIONS};
use crate::dsrn::stream::EncodedBitmap;
use crate::dsrn_exp::greedy_tiles::{decide_tiles, PlacedTile, Says};
use crate::pyramid::{tiles_across, tiles_in_level, Pyramid, CELL_LEVEL};
use crate::Bitmap;

const LEAF_WIDTH: usize = 1;
const CODE_WIDTH: usize = 1;
const FAR_WIDTH: usize = 1;
const DIRECTION_WIDTH: usize = 2;
const VALUE_WIDTH: usize = 1;
const SKIP_WIDTH: usize = 1;

const COPY: u64 = 0;
const BIND: u64 = 1;

/// What [`decide_tiles`] said about every region, by level -- a
/// region not in here was left to something finer, or is not a
/// region any placed tile lines up with.
struct TileLookup {
    says: Vec<Vec<Option<Says>>>,
}

impl TileLookup {
    fn build(pyramid: &Pyramid, bitmap: &Bitmap) -> Self {
        let mut says: Vec<Vec<Option<Says>>> =
            (0..=CELL_LEVEL).map(|level| vec![None; tiles_in_level(level)]).collect();
        for PlacedTile { region, says: what } in decide_tiles(pyramid, bitmap) {
            let across = tiles_across(region.level);
            says[region.level][region.y * across + region.x] = Some(what);
        }
        Self { says }
    }

    fn get(&self, region: Region) -> Option<Says> {
        let across = tiles_across(region.level);
        self.says[region.level][region.y * across + region.x]
    }
}

/// Encodes a bitmap: the tiles [`decide_tiles`] already found, said in
/// as few structural bits as reaching each one costs, then one raw bit
/// for every cell that leaves uncovered.
pub fn encode(pyramid: &Pyramid, bitmap: &Bitmap) -> EncodedBitmap {
    let mut out = EncodedBitmap::default();
    let lookup = TileLookup::build(pyramid, bitmap);
    let mut covered = Bitmap::new();
    encode_region(&lookup, Region::whole_bitmap(), &mut covered, &mut out);

    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if !covered.get(x, y) {
                out.push_value(bitmap.get(x, y) as u64, 1);
            }
        }
    }
    out
}

fn encode_region(lookup: &TileLookup, region: Region, covered: &mut Bitmap, out: &mut EncodedBitmap) {
    if region.level == CELL_LEVEL - 1 {
        if let Some(Says::Bound(value)) = lookup.get(region) {
            out.push_value(1, LEAF_WIDTH);
            out.push_value(value as u64, VALUE_WIDTH);
            mark_covered(covered, region);
        } else {
            // Either copyable only, or decide_tiles only reached it
            // with 1x1 tiles beneath -- either way, a hole.
            out.push_value(0, LEAF_WIDTH);
        }
        return;
    }

    match lookup.get(region) {
        Some(Says::Bound(value)) => {
            out.push_value(1, LEAF_WIDTH);
            out.push_value(BIND, CODE_WIDTH);
            out.push_value(value as u64, VALUE_WIDTH);
            mark_covered(covered, region);
        }
        Some(Says::Copied { far, direction }) => {
            out.push_value(1, LEAF_WIDTH);
            out.push_value(COPY, CODE_WIDTH);
            out.push_value(far as u64, FAR_WIDTH);
            out.push_value(direction as u64, DIRECTION_WIDTH);
            mark_covered(covered, region);
        }
        None => {
            out.push_value(0, LEAF_WIDTH);
            // One level above cells is the finest this tree tracks, so
            // a region two levels above cells has nothing to jump past
            // -- its children are handled by the 2x2 case directly,
            // and there is no choice of depth left to make.
            if region.level < CELL_LEVEL - 2 {
                let children = region.children();
                let skip_two = children.iter().all(|&child| lookup.get(child).is_none());
                out.push_value(skip_two as u64, SKIP_WIDTH);
                if skip_two {
                    // None of the four children is itself a placed
                    // tile, so all four would only say "subdivide"
                    // too -- one shared bit says that once instead of
                    // four separate ones saying it apart.
                    for child in children {
                        for grandchild in child.children() {
                            encode_region(lookup, grandchild, covered, out);
                        }
                    }
                } else {
                    for child in children {
                        encode_region(lookup, child, covered, out);
                    }
                }
            } else {
                for child in region.children() {
                    encode_region(lookup, child, covered, out);
                }
            }
        }
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
    let leaf = stream.take(*at, LEAF_WIDTH) != 0;
    *at += LEAF_WIDTH;

    if region.level == CELL_LEVEL - 1 {
        if leaf {
            let value = stream.take(*at, VALUE_WIDTH) != 0;
            *at += VALUE_WIDTH;
            mark_owner(owner, region, Owner::Bound(value));
            mark_covered(covered, region);
        }
        return;
    }

    if !leaf {
        if region.level < CELL_LEVEL - 2 {
            let skip_two = stream.take(*at, SKIP_WIDTH) != 0;
            *at += SKIP_WIDTH;
            if skip_two {
                for child in region.children() {
                    for grandchild in child.children() {
                        decode_region(stream, at, grandchild, covered, owner);
                    }
                }
            } else {
                for child in region.children() {
                    decode_region(stream, at, child, covered, owner);
                }
            }
        } else {
            for child in region.children() {
                decode_region(stream, at, child, covered, owner);
            }
        }
        return;
    }

    let code = stream.take(*at, CODE_WIDTH);
    *at += CODE_WIDTH;
    if code == BIND {
        let value = stream.take(*at, VALUE_WIDTH) != 0;
        *at += VALUE_WIDTH;
        mark_owner(owner, region, Owner::Bound(value));
    } else {
        let far = stream.take(*at, FAR_WIDTH) != 0;
        *at += FAR_WIDTH;
        let direction = stream.take(*at, DIRECTION_WIDTH) as usize;
        *at += DIRECTION_WIDTH;
        mark_owner(owner, region, Owner::Copied { direction, side: region.side_in_cells(), far });
    }
    mark_covered(covered, region);
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
