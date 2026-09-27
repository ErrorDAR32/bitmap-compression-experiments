//! Pairing the greedy pass's already-perfect tiling with a quadtree
//! that just has to say where each tile is.
//!
//! The tiles themselves are never decided here -- [`decide_tiles`]
//! already found the biggest-first, best set of homogeneous and
//! copyable tiles once, for the whole bitmap, and [`compose_complex_tiles`]
//! already decided which of them group into a complex tile; both sets
//! are treated as fixed. All this file adds is a cheap way to say
//! *where* each one is: a plain quadtree, subdivided only where it has
//! to be to reach a tile that isn't the whole of the region it would
//! otherwise bind. Reaching a region that is itself exactly one placed
//! tile costs one bit to say so, whatever that tile's size; a region
//! that isn't one whole tile costs one bit to say it subdivides, then
//! the same question again for each of its four children. Nothing here
//! searches for a size or compares a payload against leftover cells --
//! the answer for every region is already known from the tile set, so
//! building the tree is a lookup, not a decision.
//!
//! # The grammar
//!
//! ```text
//! (at any level down to one above cells)
//! 1: leaf -- this region is exactly one placed tile
//!    0: copy   + 1 far/near bit + 2 direction bits
//!    1: bind
//!       0: simple  -- 1 value bit
//!       1: complex -- this region's own four children, each its own
//!                     node (below)
//! 0: subdivide -- recurse into all four children
//!
//! (one level above cells, in place of the above)
//! 1: this 2x2 is a homogeneous placed tile + 1 value bit
//! 0: it is not -- its four cells are holes, no further bits
//!
//! (a complex tile's own node, for a region above a cell)
//! 1: leaf
//!    0: masked -- this region, described as itself (the region
//!                 grammar above), never as another complex tile
//!    1: unmasked -- 1 value bit, covering this whole region
//! 0: subdivide -- the same question asked again of this region's own
//!                 four children
//!
//! (a complex tile's own node, for a cell)
//! 1 value bit, nothing else -- masking one never pays for itself, so
//! this is always what a cell's own node turns out to be
//! ```
//!
//! A complex tile is an aligned area [`compose_complex_tiles`] found
//! cheaper to describe as one header than to leave to ordinary
//! subdivision -- see its own doc comment, and `greedy_tiles::compute`'s,
//! for how that is decided. Once chosen, a complex tile has no
//! resolution or size of its own to name: each of its four children is
//! its own recursive node, subdividing only as far as it needs to
//! before either declaring one value for the whole of whatever area it
//! stopped at, or handing the rest back to the ordinary region grammar.
//! Nothing under a complex tile's own node can be another complex tile
//! -- nesting is forbidden, mirrored here as `complex_allowed = false`
//! threaded through every region a node hands back.
//!
//! One level above cells never offers copy, and never subdivides
//! further, since cells are not tracked by the *region* grammar at all
//! (a complex tile's own node grammar is not so restricted, and can
//! reach individual cells if that is where it settles): a 2x2 that is
//! only copyable, or that decide_tiles only managed to cover with 1x1
//! tiles, is left entirely to the trailing raw pass, at one bit a cell
//! -- cheaper than a copy's own header for one tile that small, and
//! there is nothing finer here to subdivide into.
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
use crate::dsrn_exp::greedy_tiles::{
    compose_complex_tiles, decide_tiles, Node, PlacedTile, Says, CODE_WIDTH, COMPLEX_FLAG_WIDTH, DIRECTION_WIDTH,
    FAR_WIDTH, LEAF_WIDTH, NODE_LEAF_WIDTH, NODE_STATE_WIDTH, VALUE_WIDTH,
};
use crate::pyramid::{tiles_across, tiles_in_level, Pyramid, CELL_LEVEL};
use crate::Bitmap;

const COPY: u64 = 0;
const BIND: u64 = 1;
const SIMPLE: u64 = 0;
const COMPLEX: u64 = 1;

/// A [`Node`] above a cell: leaf (decide masked or unmasked right
/// here) or subdivide (ask the same of this area's own four children,
/// at half the size). A cell skips this bit entirely -- there is
/// nothing finer to subdivide into, and masking one never pays for
/// itself either, so `greedy_tiles::compute` never produces anything
/// but a plain leaf there.
const NODE_SUBDIVIDE: u64 = 0;
const NODE_LEAF: u64 = 1;

/// A [`Node`] leaf's own decision.
const NODE_MASKED: u64 = 0;
const NODE_UNMASKED: u64 = 1;

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
        for PlacedTile { region, says: what } in compose_complex_tiles(decide_tiles(pyramid, bitmap)) {
            let across = tiles_across(region.level);
            says[region.level][region.y * across + region.x] = Some(what);
        }
        Self { says }
    }

    fn get(&self, region: Region) -> Option<Says> {
        let across = tiles_across(region.level);
        self.says[region.level][region.y * across + region.x].clone()
    }
}

/// Encodes a bitmap: the tiles [`decide_tiles`] already found, said in
/// as few structural bits as reaching each one costs, then one raw bit
/// for every cell that leaves uncovered.
pub fn encode(pyramid: &Pyramid, bitmap: &Bitmap) -> EncodedBitmap {
    let mut out = EncodedBitmap::default();
    let lookup = TileLookup::build(pyramid, bitmap);
    let mut covered = Bitmap::new();
    encode_region(&lookup, Region::whole_bitmap(), &mut covered, &mut out, true);

    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if !covered.get(x, y) {
                out.push_value(bitmap.get(x, y) as u64, 1);
            }
        }
    }
    out
}

/// `complex_allowed` is `false` anywhere under a complex tile's own
/// node tree -- nesting a complex tile there is forbidden, so a region
/// there can only ever be `Bound`, `Copied`, or subdivided, never
/// `Complex`, and the complex-flag bit every bind would otherwise pay
/// is skipped entirely: there is nothing left for it to distinguish.
fn encode_region(lookup: &TileLookup, region: Region, covered: &mut Bitmap, out: &mut EncodedBitmap, complex_allowed: bool) {
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
            if complex_allowed {
                out.push_value(SIMPLE, COMPLEX_FLAG_WIDTH);
            }
            out.push_value(value as u64, VALUE_WIDTH);
            mark_covered(covered, region);
        }
        Some(Says::Complex(nodes)) => {
            debug_assert!(complex_allowed, "a complex tile can never nest inside another's mask");
            out.push_value(1, LEAF_WIDTH);
            out.push_value(BIND, CODE_WIDTH);
            out.push_value(COMPLEX, COMPLEX_FLAG_WIDTH);
            for (child, node) in region.children().into_iter().zip(nodes.iter()) {
                encode_node(lookup, child, node, covered, out);
            }
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
            for child in region.children() {
                encode_region(lookup, child, covered, out, complex_allowed);
            }
        }
    }
}

/// Writes one [`Node`] of a complex tile's own subtree, for `region`.
/// `Leaf` writes its one value right here, covering the whole of
/// `region` however big it turned out to be. `Masked` writes `region`
/// as an ordinary region instead, with nesting a complex tile inside
/// it forbidden. `Subdivided` asks the same of `region`'s own four
/// children, at half the size -- which is how a node reaches all the
/// way down to individual cells when that turns out cheapest, with no
/// resolution or size announced anywhere to say how far it went. A
/// cell (`region.level == CELL_LEVEL`) skips the leaf/subdivide and
/// masked/unmasked bits entirely: see `greedy_tiles::compute`'s own
/// doc comment for why a cell is always a plain leaf.
fn encode_node(lookup: &TileLookup, region: Region, node: &Node, covered: &mut Bitmap, out: &mut EncodedBitmap) {
    if region.level == CELL_LEVEL {
        let Node::Leaf(value) = node else { unreachable!("a cell is always a leaf") };
        out.push_value(*value as u64, VALUE_WIDTH);
        mark_covered(covered, region);
        return;
    }

    match node {
        Node::Subdivided(children) => {
            out.push_value(NODE_SUBDIVIDE, NODE_LEAF_WIDTH);
            for (child, node) in region.children().into_iter().zip(children.iter()) {
                encode_node(lookup, child, node, covered, out);
            }
        }
        Node::Leaf(value) => {
            out.push_value(NODE_LEAF, NODE_LEAF_WIDTH);
            out.push_value(NODE_UNMASKED, NODE_STATE_WIDTH);
            out.push_value(*value as u64, VALUE_WIDTH);
            mark_covered(covered, region);
        }
        Node::Masked => {
            out.push_value(NODE_LEAF, NODE_LEAF_WIDTH);
            out.push_value(NODE_MASKED, NODE_STATE_WIDTH);
            encode_region(lookup, region, covered, out, false);
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
    decode_region(stream, &mut at, Region::whole_bitmap(), &mut covered, &mut owner, true);

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

/// `complex_allowed` mirrors [`encode_region`]'s own: `false` anywhere
/// under a complex tile's own node tree, where the complex-flag bit
/// was never written at all, so it must not be read either.
fn decode_region(
    stream: &EncodedBitmap,
    at: &mut usize,
    region: Region,
    covered: &mut Bitmap,
    owner: &mut [Option<Owner>],
    complex_allowed: bool,
) {
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
        for child in region.children() {
            decode_region(stream, at, child, covered, owner, complex_allowed);
        }
        return;
    }

    let code = stream.take(*at, CODE_WIDTH);
    *at += CODE_WIDTH;
    if code == BIND {
        let complex = complex_allowed && stream.take(*at, COMPLEX_FLAG_WIDTH) == COMPLEX;
        if complex_allowed {
            *at += COMPLEX_FLAG_WIDTH;
        }
        if complex {
            for child in region.children() {
                decode_node(stream, at, child, covered, owner);
            }
            return;
        } else {
            let value = stream.take(*at, VALUE_WIDTH) != 0;
            *at += VALUE_WIDTH;
            mark_owner(owner, region, Owner::Bound(value));
        }
    } else {
        let far = stream.take(*at, FAR_WIDTH) != 0;
        *at += FAR_WIDTH;
        let direction = stream.take(*at, DIRECTION_WIDTH) as usize;
        *at += DIRECTION_WIDTH;
        mark_owner(owner, region, Owner::Copied { direction, side: region.side_in_cells(), far });
    }
    mark_covered(covered, region);
}

/// Reads one [`Node`], mirroring [`encode_node`] exactly.
fn decode_node(stream: &EncodedBitmap, at: &mut usize, region: Region, covered: &mut Bitmap, owner: &mut [Option<Owner>]) {
    if region.level == CELL_LEVEL {
        let value = stream.take(*at, VALUE_WIDTH) != 0;
        *at += VALUE_WIDTH;
        mark_owner(owner, region, Owner::Bound(value));
        mark_covered(covered, region);
        return;
    }

    let leaf = stream.take(*at, NODE_LEAF_WIDTH) == NODE_LEAF;
    *at += NODE_LEAF_WIDTH;
    if !leaf {
        for child in region.children() {
            decode_node(stream, at, child, covered, owner);
        }
        return;
    }

    let unmasked = stream.take(*at, NODE_STATE_WIDTH) == NODE_UNMASKED;
    *at += NODE_STATE_WIDTH;
    if unmasked {
        let value = stream.take(*at, VALUE_WIDTH) != 0;
        *at += VALUE_WIDTH;
        mark_owner(owner, region, Owner::Bound(value));
        mark_covered(covered, region);
    } else {
        decode_region(stream, at, region, covered, owner, false);
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
