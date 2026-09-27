//! [`super::tile_or_subdivide`], but the tree above 4x4 has sixteen
//! children a node instead of four.
//!
//! Same pairing, with one difference: [`decide_tiles`] is restricted
//! here to exactly the sizes this tree can make a node of --
//! [`levels`] -- rather than every size down to a cell. 128 and 32 are
//! never tried and never a node in this tree at all; a homogeneous or
//! copyable area at one of those sizes is simply found again, at one
//! size finer, by each of its own four quarters -- more tiles for that
//! area, never a lost one, since every cell is still exactly as one
//! thing as it always was. This only says *where* each tile is: the
//! only thing different from `tile_or_subdivide` is how coarse a jump
//! "not one whole tile, look closer" is allowed to be. From the whole
//! bitmap down to 4x4, one subdivide bit skips two quadtree levels at
//! once -- 256, 64, 16, 4, sixteen children a node -- instead of one.
//!
//! Below 4x4 nothing changes at all: the ordinary one-level-a-jump
//! quadtree, the same 2x2 special case, and the same trailing raw pass
//! for whatever the tree leaves uncovered, exactly as
//! `tile_or_subdivide` already does it -- "restricted to available
//! node sizes, to keep compatibility" is the whole of the idea, not a
//! reason to also rebuild the part that already works.
//!
//! # The grammar
//!
//! ```text
//! (at 256, 64 and 16)
//! 1: leaf -- this region is exactly one placed tile
//!    0: copy   + 1 far/near bit + 2 direction bits
//!    1: bind   + 1 value bit
//! 0: subdivide -- recurse into all sixteen grandchildren two levels down
//!
//! (at 4x4, in the ordinary quadtree grammar again)
//! 1: leaf, same as above
//! 0: subdivide -- recurse into all four 2x2 children
//!
//! (at 2x2, same special case as tile_or_subdivide)
//! 1: this 2x2 is a homogeneous placed tile + 1 value bit
//! 0: it is not -- its four cells are holes, no further bits
//! ```
//!
//! Node sizes are therefore restricted to 256, 64, 16, 4, 2 and 1 --
//! the last one only ever as a raw trailing bit, never a node of its
//! own, same as `tile_or_subdivide`. Decoding is the same two-pass
//! deferred resolution for the same reason: a copy is chosen on
//! content alone, so its source may not be resolved yet by the time
//! the tree reaches it.

use crate::dsrn::region::{Region, DIRECTIONS};
use crate::dsrn::stream::EncodedBitmap;
use crate::dsrn_exp::greedy_tiles::{decide_tiles, PlacedTile, Says};
use crate::dsrn_exp::tile_or_subdivide::Breakdown;
use crate::pyramid::{tiles_across, tiles_in_level, Pyramid, CELL_LEVEL};
use crate::Bitmap;

const LEAF_WIDTH: usize = 1;
const CODE_WIDTH: usize = 1;
const FAR_WIDTH: usize = 1;
const DIRECTION_WIDTH: usize = 2;
const VALUE_WIDTH: usize = 1;

const COPY: u64 = 0;
const BIND: u64 = 1;

/// How many quadtree levels one subdivide jumps, from a region at
/// this level: two above 4x4, for the sixteen-child hexadecatree
/// levels, one from 4x4 down, for the ordinary quadtree.
fn jump(level: usize) -> usize {
    if level < CELL_LEVEL - 2 {
        2
    } else {
        1
    }
}

/// The levels this tree can ever place a leaf at, coarsest first --
/// exactly the levels [`jump`] lands a subdivide on, walked from the
/// whole bitmap down to cells. [`decide_tiles`] is restricted to
/// these, so it never places a tile at a size (128, 32 or 8) this
/// tree could never make a node of, and finds the same content at a
/// size that is one of these instead.
fn levels() -> Vec<usize> {
    let mut found = vec![0];
    let mut level = 0;
    while level < CELL_LEVEL {
        level += jump(level);
        found.push(level);
    }
    found
}

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
        for PlacedTile { region, says: what } in decide_tiles(pyramid, bitmap, &levels()) {
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
    encode_with_breakdown(pyramid, bitmap).0
}

/// The same as [`encode`], with a running count of how many of its
/// bits are structure against payload -- the same split
/// [`super::tile_or_subdivide::encode_with_breakdown`] reports, so the
/// two are directly comparable.
pub fn encode_with_breakdown(pyramid: &Pyramid, bitmap: &Bitmap) -> (EncodedBitmap, Breakdown) {
    let mut out = EncodedBitmap::default();
    let mut breakdown = Breakdown::default();
    let lookup = TileLookup::build(pyramid, bitmap);
    let mut covered = Bitmap::new();
    encode_region(&lookup, Region::whole_bitmap(), &mut covered, &mut out, &mut breakdown);

    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if !covered.get(x, y) {
                out.push_value(bitmap.get(x, y) as u64, 1);
                breakdown.payload += 1;
            }
        }
    }
    (out, breakdown)
}

fn encode_region(
    lookup: &TileLookup,
    region: Region,
    covered: &mut Bitmap,
    out: &mut EncodedBitmap,
    breakdown: &mut Breakdown,
) {
    if region.level == CELL_LEVEL - 1 {
        if let Some(Says::Bound(value)) = lookup.get(region) {
            out.push_value(1, LEAF_WIDTH);
            out.push_value(value as u64, VALUE_WIDTH);
            breakdown.structure += LEAF_WIDTH;
            breakdown.payload += VALUE_WIDTH;
            mark_covered(covered, region);
        } else {
            // Either copyable only, or decide_tiles only reached it
            // with 1x1 tiles beneath -- either way, a hole.
            out.push_value(0, LEAF_WIDTH);
            breakdown.structure += LEAF_WIDTH;
        }
        return;
    }

    match lookup.get(region) {
        Some(Says::Bound(value)) => {
            out.push_value(1, LEAF_WIDTH);
            out.push_value(BIND, CODE_WIDTH);
            out.push_value(value as u64, VALUE_WIDTH);
            breakdown.structure += LEAF_WIDTH + CODE_WIDTH;
            breakdown.payload += VALUE_WIDTH;
            mark_covered(covered, region);
        }
        Some(Says::Copied { far, direction }) => {
            out.push_value(1, LEAF_WIDTH);
            out.push_value(COPY, CODE_WIDTH);
            out.push_value(far as u64, FAR_WIDTH);
            out.push_value(direction as u64, DIRECTION_WIDTH);
            breakdown.structure += LEAF_WIDTH + CODE_WIDTH + FAR_WIDTH + DIRECTION_WIDTH;
            mark_covered(covered, region);
        }
        None => {
            out.push_value(0, LEAF_WIDTH);
            breakdown.structure += LEAF_WIDTH;
            for child in region.tiles_at_depth(jump(region.level)) {
                encode_region(lookup, child, covered, out, breakdown);
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
        for child in region.tiles_at_depth(jump(region.level)) {
            decode_region(stream, at, child, covered, owner);
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

/// Bits against dsrn's own, and against [`super::tile_or_subdivide`],
/// in the same currency both already compare in.
pub fn run() {
    use crate::dsrn::{encode as dsrn_encode, FourByFour, Knobs, Masking, Workspace};
    use crate::samples;

    let knobs = Knobs { masking: Masking::Anywhere, four_by_four: FourByFour::ItsOwnGrammar };
    let (mut pyramid, mut work, mut dsrn_out) =
        (Pyramid::new(), Workspace::new(), crate::dsrn::Encoded::default());

    for (family, maps) in samples::every_family() {
        let mut dsrn_bits = 0usize;
        let (mut dsrn_tree, mut dsrn_payload) = (0usize, 0usize);
        let mut quad = Breakdown::default();
        let mut hexadeca = Breakdown::default();
        for bitmap in &maps {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            dsrn_encode(&pyramid, bitmap, knobs, &mut work, &mut dsrn_out);
            dsrn_bits += dsrn_out.bits();
            dsrn_tree += dsrn_out.tree.len();
            dsrn_payload += dsrn_out.payload.len();
            let (_, quad_breakdown) = super::tile_or_subdivide::encode_with_breakdown(&pyramid, bitmap);
            let (_, hexadeca_breakdown) = encode_with_breakdown(&pyramid, bitmap);
            quad.structure += quad_breakdown.structure;
            quad.payload += quad_breakdown.payload;
            hexadeca.structure += hexadeca_breakdown.structure;
            hexadeca.payload += hexadeca_breakdown.payload;
        }
        let n = maps.len();
        let quad_bits = quad.total();
        let hexadeca_bits = hexadeca.total();
        println!(
            "\n  {family}, {n} bitmaps: dsrn {} bits a bitmap, tile-or-subdivide {} ({:+.1}%), hexadecatree {} ({:+.1}%)",
            dsrn_bits / n,
            quad_bits / n,
            100.0 * (quad_bits as f64 - dsrn_bits as f64) / dsrn_bits as f64,
            hexadeca_bits / n,
            100.0 * (hexadeca_bits as f64 - dsrn_bits as f64) / dsrn_bits as f64
        );
        println!(
            "    structure vs payload, a bitmap: dsrn {}/{} ({:.1}% structure), tile-or-subdivide {}/{} ({:.1}%), hexadecatree {}/{} ({:.1}%)",
            dsrn_tree / n,
            dsrn_payload / n,
            100.0 * dsrn_tree as f64 / dsrn_bits as f64,
            quad.structure / n,
            quad.payload / n,
            100.0 * quad.structure as f64 / quad_bits as f64,
            hexadeca.structure / n,
            hexadeca.payload / n,
            100.0 * hexadeca.structure as f64 / hexadeca_bits as f64
        );
    }
}
