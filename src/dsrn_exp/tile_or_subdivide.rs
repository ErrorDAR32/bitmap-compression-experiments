//! Walking the tree the greedy complex tiler already decided, to write
//! or read a bitstream -- nothing here ever decides anything. Building
//! that tree is entirely `greedy_tiles.rs`'s own job (`greedy_tiler`
//! then `complex_tiler`, producing one whole-bitmap [`Node`]); this
//! file only says how a [`Node`], in whatever [`Context`] it sits in,
//! is spelled out in bits, and how to read that back -- "build trees
//! first, then traverse the tree to encode," never combined.
//!
//! # The grammar
//!
//! One shape, reused everywhere: a leaf-or-subdivide bit, and, at a
//! leaf, a bind-or-copy bit. A complex tile's own body ([`write_body_node`])
//! is not a second grammar bolted on -- it is this exact same shape,
//! with the one addition a body actually needs: a `bind` leaf can be a
//! *run* of more than one resolution-tile value, not just its own
//! single one, so above the resolution a `bind` leaf spends one further
//! bit saying which. Already at the resolution a run is never more than
//! one value long, so that bit is never spent there -- free context,
//! the same reasoning [`resolution_width`] already relies on.
//!
//! ```text
//! Open or Plain, at any level down to one above cells:
//! 1: leaf -- this region is exactly one placed tile
//!    0: copy   + 1 far/near bit + 2 direction bits
//!    1: bind
//!       0: simple  -- 1 value bit
//!       1: complex (Open only) -- resolution_width(level) bits (depth - 1),
//!             then the complex tile's own body (below)
//! 0: subdivide -- recurse into all four children, same context
//!
//! Open or Plain, one level above cells, in place of the above:
//! 1: this 2x2 is a homogeneous placed tile + 1 value bit
//! 0: it is not -- its four cells are holes, no further bits
//!
//! A complex tile's own body, right after its resolution bits, only
//! when depth > 1 (depth 1 never masks, so this is skipped there,
//! known on both sides to mean "no masking" for free):
//! 0: the whole of the body is flat -- one value bit a resolution
//!    tile, 4^depth of them, in reading order
//! 1: masked somewhere -- one body node (below) for each of the
//!    complex tile's own four direct children
//!
//! One body node, above the complex tile's own resolution:
//! 1: leaf
//!    0: copy   + 1 far/near bit + 2 direction bits
//!    1: bind
//!       0: this one placed tile's own single value -- masked whole,
//!          bigger than the resolution
//!       1: flat -- a run of resolution-tile values below this node,
//!          in reading order
//! 0: subdivide -- recurse into this node's own four children
//!
//! One body node, already at the complex tile's own resolution: the
//! same shape, minus the run-length bit (never needed here -- a leaf
//! this deep is always exactly one value):
//! 1: leaf
//!    0: copy, masked -- + 1 far/near bit + 2 direction bits
//!    1: bind -- this one resolution tile's own value
//! 0: subdivide
//!    (resolution 2x2 only: nothing more to read -- a hole, the
//!    top-level tree's own floor)
//!    (otherwise: finer content below -- four `Plain` children follow)
//! ```
//!
//! Whatever the tree never covers -- every hole a 2x2 leaves, and
//! nothing else -- gets exactly one raw bit a cell, in reading order,
//! appended once the whole tree is written. [`claim_owners`] is the one
//! function that says which cells those are, shared by both directions
//! so the trailing pass's own reading order can never disagree between
//! them.
//!
//! Decoding is two passes, exactly like [`crate::dsrn_exp::tile_stream`]:
//! a copy is chosen on content alone by `greedy_tiler`, so its source
//! may not be resolved yet by the time the tree reaches it, and may
//! even be a hole the trailing pass has not read yet. [`parse_node`]
//! reads the whole tree with no value resolution at all; [`claim_owners`]
//! then walks that already-built tree, recording every cell as a value
//! or a direction to read one from; the trailing raw bits fill in
//! whatever is left; then [`resolve`] resolves by repeated sweeps,
//! deferring a copy to the next one whenever its source is not resolved
//! yet. No cycle is possible -- a copy always names something reading
//! order puts before it -- so this always finishes, backed by an
//! assertion rather than blind trust.

use crate::dsrn::region::{deepest_depth, Region, DIRECTIONS};
use crate::dsrn::stream::EncodedBitmap;
use crate::dsrn_exp::greedy_tiles::{complex_tiler, greedy_tiler, Context, Node};
use crate::pyramid::Pyramid;
use crate::Bitmap;

const LEAF_WIDTH: usize = 1;
const CODE_WIDTH: usize = 1;
const FAR_WIDTH: usize = 1;
const DIRECTION_WIDTH: usize = 2;
const VALUE_WIDTH: usize = 1;
const COMPLEX_FLAG_WIDTH: usize = 1;

const COPY: u64 = 0;
const BIND: u64 = 1;
const SIMPLE: u64 = 0;
const COMPLEX: u64 = 1;

/// Whether a complex tile's own top-level body is flat outright -- `0`
/// skips straight to the shared value list every complex tile used to
/// be, `1` means at least something inside needs masking. Skipped
/// entirely at depth 1, where masking never pays for itself and the
/// body is always flat, so this bit would carry no information there.
const MASK_PRESENT_WIDTH: usize = 1;
const NO_MASKING: u64 = 0;
const MASKING: u64 = 1;

/// A body node reuses the exact same leaf-or-subdivide, then
/// bind-or-copy shape [`write_node`] already has for `Open`/`Plain` --
/// `LEAF_WIDTH`, `CODE_WIDTH`, `BIND` and `COPY` all mean exactly what
/// they do there. The one thing a body node can say that an ordinary
/// leaf cannot is that a `bind` leaf is a *run* of more than one
/// resolution-tile value rather than just its own single one, so a
/// `bind` leaf above the resolution spends one further bit to say
/// which. Already at the resolution a run is never more than one tile
/// long, so nothing there can ever need that bit, the same "free
/// context, no bit to spend" reasoning [`resolution_width`] already
/// relies on.
const FLAT_WIDTH: usize = 1;
const FLAT: u64 = 1;
const SINGLE: u64 = 0;

/// How many bits it takes to name a `depth - 1` value at `region.level`
/// -- a complex tile's own resolution field, sized to what a region at
/// that level could actually need rather than a flat width everywhere.
/// `depth` never exceeds [`deepest_depth`] (there is nothing finer than
/// a cell to decompose into), so `depth - 1` never exceeds
/// `deepest_depth - 1`, and a region's own level is already known from
/// its place in the tree -- free context, not a bit anyone has to
/// spend.
fn resolution_width(level: usize) -> usize {
    bits_to_name(deepest_depth(level))
}

/// How many bits it takes to name one of `count` values, `0`-indexed --
/// `0` when there is only one, since nothing is left to say.
fn bits_to_name(count: usize) -> usize {
    let mut bits = 0;
    while (1usize << bits) < count {
        bits += 1;
    }
    bits
}

/// Encodes a bitmap: `greedy_tiler` then `complex_tiler`'s own tree,
/// said in as few structural bits as reaching each of its nodes costs,
/// then one raw bit for every cell the tree leaves uncovered.
pub fn encode(pyramid: &Pyramid, bitmap: &Bitmap) -> EncodedBitmap {
    let tree = complex_tiler(&greedy_tiler(pyramid, bitmap));
    let mut out = EncodedBitmap::default();
    write_node(&tree, Region::whole_bitmap(), Context::Open, &mut out);

    let mut owner: Vec<Option<Owner>> = vec![None; 256 * 256];
    let mut covered = Bitmap::new();
    claim_owners(&tree, Region::whole_bitmap(), Context::Open, &mut owner, &mut covered);
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if !covered.get(x, y) {
                out.push_value(bitmap.get(x, y) as u64, 1);
            }
        }
    }
    out
}

/// Writes one [`Node`] in `context`, for `region`.
fn write_node(node: &Node, region: Region, context: Context, out: &mut EncodedBitmap) {
    use crate::pyramid::CELL_LEVEL;

    if region.level == CELL_LEVEL - 1 {
        match node {
            Node::Bound(value) => {
                out.push_value(1, LEAF_WIDTH);
                out.push_value(*value as u64, VALUE_WIDTH);
            }
            Node::Hole => out.push_value(0, LEAF_WIDTH),
            _ => unreachable!("one level above cells is always Bound or Hole"),
        }
        return;
    }

    match node {
        Node::Bound(value) => {
            out.push_value(1, LEAF_WIDTH);
            out.push_value(BIND, CODE_WIDTH);
            if context == Context::Open {
                out.push_value(SIMPLE, COMPLEX_FLAG_WIDTH);
            }
            out.push_value(*value as u64, VALUE_WIDTH);
        }
        Node::Copied { far, direction } => {
            out.push_value(1, LEAF_WIDTH);
            out.push_value(COPY, CODE_WIDTH);
            out.push_value(*far as u64, FAR_WIDTH);
            out.push_value(*direction as u64, DIRECTION_WIDTH);
        }
        Node::Complex { depth, body } => {
            debug_assert_eq!(context, Context::Open, "a complex tile can never nest inside another's body");
            out.push_value(1, LEAF_WIDTH);
            out.push_value(BIND, CODE_WIDTH);
            out.push_value(COMPLEX, COMPLEX_FLAG_WIDTH);
            out.push_value((*depth - 1) as u64, resolution_width(region.level));
            write_complex_body(body, region, *depth, region.level + *depth, out);
        }
        Node::Split(children) => {
            out.push_value(0, LEAF_WIDTH);
            for (child_region, child_node) in region.children().into_iter().zip(children.iter()) {
                write_node(child_node, child_region, context, out);
            }
        }
        Node::Flat(_) => unreachable!("Flat only ever appears inside a complex tile's own body"),
        Node::Hole => unreachable!("Hole only ever appears one level above cells"),
    }
}

/// Writes a complex tile's own top-level body -- `region` is the
/// complex tile's own region, not one of its children. Skips the
/// mask-present bit entirely at `depth == 1` (masking never pays for
/// itself there, so the body is always `Flat` and the bit would carry
/// no information); otherwise writes it, then either the whole flat
/// value list or, one node per direct child, [`write_body_node`].
fn write_complex_body(body: &Node, region: Region, depth: usize, limit_level: usize, out: &mut EncodedBitmap) {
    if depth == 1 {
        let Node::Flat(values) = body else { unreachable!("masking never pays for itself at depth 1") };
        for &value in values {
            out.push_value(value as u64, VALUE_WIDTH);
        }
        return;
    }
    match body {
        Node::Flat(values) => {
            out.push_value(NO_MASKING, MASK_PRESENT_WIDTH);
            for &value in values {
                out.push_value(value as u64, VALUE_WIDTH);
            }
        }
        Node::Split(children) => {
            out.push_value(MASKING, MASK_PRESENT_WIDTH);
            for (child_region, child_node) in region.children().into_iter().zip(children.iter()) {
                write_body_node(child_node, child_region, limit_level, out);
            }
        }
        _ => unreachable!("a complex tile's own top-level body is always Flat or Split"),
    }
}

/// Writes one body node, for `region`, down to `limit_level` -- the
/// complex tile's own resolution, the finest a body node ever goes.
/// Exactly the ordinary leaf-or-subdivide, bind-or-copy grammar
/// [`write_node`] already has: `LEAF_WIDTH` then, at a leaf,
/// `CODE_WIDTH`. The only new thing a `bind` leaf can say is that it is
/// a *run* of resolution-tile values rather than one placed tile's own
/// single value -- [`FLAT_WIDTH`], spent only above the resolution,
/// where a run longer than one is actually possible.
///
/// Already at the resolution (`region.level == limit_level`),
/// `subdivide` does not mean "more body nodes" -- there is nothing
/// finer left for the complex tile itself to say -- it means finer
/// content the resolution does not reach, read by the ordinary tree in
/// `Context::Plain`, exactly like crossing into a region [`write_node`]
/// was never told to expect a complex tile inside. At the 2x2 floor
/// specifically that finer content is never read as a tree node at all
/// (the top-level tree's own floor, nothing below 2x2 ever is), so
/// `subdivide` there is simply a hole, no further bits.
fn write_body_node(node: &Node, region: Region, limit_level: usize, out: &mut EncodedBitmap) {
    use crate::pyramid::CELL_LEVEL;

    let above_resolution = region.level < limit_level;
    match node {
        Node::Bound(value) => {
            out.push_value(1, LEAF_WIDTH);
            out.push_value(BIND, CODE_WIDTH);
            if above_resolution {
                out.push_value(SINGLE, FLAT_WIDTH);
            }
            out.push_value(*value as u64, VALUE_WIDTH);
        }
        Node::Flat(values) => {
            debug_assert!(above_resolution, "a run at the resolution itself is never more than one value long");
            out.push_value(1, LEAF_WIDTH);
            out.push_value(BIND, CODE_WIDTH);
            out.push_value(FLAT, FLAT_WIDTH);
            for &value in values {
                out.push_value(value as u64, VALUE_WIDTH);
            }
        }
        Node::Copied { far, direction } => {
            out.push_value(1, LEAF_WIDTH);
            out.push_value(COPY, CODE_WIDTH);
            out.push_value(*far as u64, FAR_WIDTH);
            out.push_value(*direction as u64, DIRECTION_WIDTH);
        }
        Node::Hole => {
            debug_assert!(!above_resolution && limit_level == CELL_LEVEL - 1, "a hole only ever appears at the 2x2 floor");
            out.push_value(0, LEAF_WIDTH);
        }
        Node::Split(children) => {
            out.push_value(0, LEAF_WIDTH);
            if above_resolution {
                for (child_region, child_node) in region.children().into_iter().zip(children.iter()) {
                    write_body_node(child_node, child_region, limit_level, out);
                }
            } else {
                for (child_region, child_node) in region.children().into_iter().zip(children.iter()) {
                    write_node(child_node, child_region, Context::Plain, out);
                }
            }
        }
        Node::Complex { .. } => unreachable!("a complex tile can never nest inside another's body"),
    }
}

/// What a cell's owner says, once the tree and the trailing raw pass
/// have both been read: its own value, or a direction and distance to
/// read the matching cell from.
#[derive(Clone, Copy)]
enum Owner {
    Bound(bool),
    Copied { direction: usize, side: usize, far: bool },
}

/// Walks an already-built [`Node`] tree, recording every cell it covers
/// as a value or a direction to read one from, and marking `covered`
/// to match. The one function both `encode` (to find which cells the
/// tree leaves for the trailing raw pass) and `decode` (to actually
/// resolve them) use, so the two can never disagree about which cells
/// those are or in what order.
fn claim_owners(node: &Node, region: Region, context: Context, owner: &mut [Option<Owner>], covered: &mut Bitmap) {
    match node {
        Node::Bound(value) => {
            mark_owner(owner, region, Owner::Bound(*value));
            mark_covered(covered, region);
        }
        Node::Copied { far, direction } => {
            mark_owner(owner, region, Owner::Copied { direction: *direction, side: region.side_in_cells(), far: *far });
            mark_covered(covered, region);
        }
        Node::Hole => {} // its cells are left entirely to the trailing raw pass
        Node::Flat(values) => {
            let Context::Body { limit_level } = context else {
                unreachable!("Flat only ever appears inside a complex tile's own body")
            };
            for (tile, &value) in region.tiles_at_depth(limit_level - region.level).into_iter().zip(values) {
                mark_owner(owner, tile, Owner::Bound(value));
            }
            mark_covered(covered, region);
        }
        Node::Complex { depth, body } => {
            claim_owners(body, region, Context::Body { limit_level: region.level + depth }, owner, covered);
        }
        Node::Split(children) => {
            // The crossover from a complex tile's own body into `Plain`
            // happens once *this* node -- the one doing the splitting
            // -- is already at the resolution: that is exactly when
            // `build_body_node` itself switches to building its own
            // children with `build_node(..., Context::Plain)` instead
            // of recursing into more body nodes, and `write_body_node`
            // makes the same check against its own `region`, never the
            // child's.
            let child_context = match context {
                Context::Body { limit_level } if region.level == limit_level => Context::Plain,
                other => other,
            };
            for (child_region, child_node) in region.children().into_iter().zip(children.iter()) {
                claim_owners(child_node, child_region, child_context, owner, covered);
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

/// Decodes a stream written by [`encode`]: parses the whole tree first,
/// with no value resolution at all, then a separate pass
/// ([`claim_owners`]) reads it back into cell owners, then the trailing
/// raw bits fill in whatever is left, then [`resolve`] resolves every
/// cell.
pub fn decode(stream: &EncodedBitmap) -> Bitmap {
    let mut at = 0usize;
    let tree = parse_node(stream, &mut at, Region::whole_bitmap(), Context::Open);

    let mut owner: Vec<Option<Owner>> = vec![None; 256 * 256];
    let mut covered = Bitmap::new();
    claim_owners(&tree, Region::whole_bitmap(), Context::Open, &mut owner, &mut covered);

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

/// Reads one [`Node`] in `context`, for `region`, mirroring
/// [`write_node`] exactly.
fn parse_node(stream: &EncodedBitmap, at: &mut usize, region: Region, context: Context) -> Node {
    use crate::pyramid::CELL_LEVEL;

    let leaf = stream.take(*at, LEAF_WIDTH) != 0;
    *at += LEAF_WIDTH;

    if region.level == CELL_LEVEL - 1 {
        if leaf {
            let value = stream.take(*at, VALUE_WIDTH) != 0;
            *at += VALUE_WIDTH;
            return Node::Bound(value);
        }
        return Node::Hole;
    }

    if !leaf {
        return Node::Split(Box::new(region.children().map(|child| parse_node(stream, at, child, context))));
    }

    let code = stream.take(*at, CODE_WIDTH);
    *at += CODE_WIDTH;
    if code == BIND {
        let complex = context == Context::Open && {
            let complex = stream.take(*at, COMPLEX_FLAG_WIDTH) == COMPLEX;
            *at += COMPLEX_FLAG_WIDTH;
            complex
        };
        if complex {
            let width = resolution_width(region.level);
            let depth = stream.take(*at, width) as usize + 1;
            *at += width;
            let body = parse_complex_body(stream, at, region, depth, region.level + depth);
            Node::Complex { depth, body: Box::new(body) }
        } else {
            let value = stream.take(*at, VALUE_WIDTH) != 0;
            *at += VALUE_WIDTH;
            Node::Bound(value)
        }
    } else {
        let far = stream.take(*at, FAR_WIDTH) != 0;
        *at += FAR_WIDTH;
        let direction = stream.take(*at, DIRECTION_WIDTH) as usize;
        *at += DIRECTION_WIDTH;
        Node::Copied { far, direction }
    }
}

/// Reads a complex tile's own top-level body, mirroring
/// [`write_complex_body`] exactly.
fn parse_complex_body(stream: &EncodedBitmap, at: &mut usize, region: Region, depth: usize, limit_level: usize) -> Node {
    if depth == 1 {
        let values = (0..4).map(|_| read_value(stream, at)).collect();
        return Node::Flat(values);
    }
    let masking = stream.take(*at, MASK_PRESENT_WIDTH) == MASKING;
    *at += MASK_PRESENT_WIDTH;
    if !masking {
        let count = 1usize << (2 * depth);
        let values = (0..count).map(|_| read_value(stream, at)).collect();
        return Node::Flat(values);
    }
    Node::Split(Box::new(region.children().map(|child| parse_body_node(stream, at, child, limit_level))))
}

/// Reads one body node, mirroring [`write_body_node`] exactly.
fn parse_body_node(stream: &EncodedBitmap, at: &mut usize, region: Region, limit_level: usize) -> Node {
    use crate::pyramid::CELL_LEVEL;

    let above_resolution = region.level < limit_level;
    let leaf = stream.take(*at, LEAF_WIDTH) != 0;
    *at += LEAF_WIDTH;

    if !leaf {
        if above_resolution {
            return Node::Split(Box::new(region.children().map(|child| parse_body_node(stream, at, child, limit_level))));
        }
        if limit_level == CELL_LEVEL - 1 {
            return Node::Hole;
        }
        return Node::Split(Box::new(region.children().map(|child| parse_node(stream, at, child, Context::Plain))));
    }

    let code = stream.take(*at, CODE_WIDTH);
    *at += CODE_WIDTH;
    if code == BIND {
        if above_resolution {
            let flat = stream.take(*at, FLAT_WIDTH) == FLAT;
            *at += FLAT_WIDTH;
            if flat {
                let count = 1usize << (2 * (limit_level - region.level));
                let values = (0..count).map(|_| read_value(stream, at)).collect();
                return Node::Flat(values);
            }
        }
        Node::Bound(read_value(stream, at))
    } else {
        let far = stream.take(*at, FAR_WIDTH) != 0;
        *at += FAR_WIDTH;
        let direction = stream.take(*at, DIRECTION_WIDTH) as usize;
        *at += DIRECTION_WIDTH;
        Node::Copied { far, direction }
    }
}

/// Reads one value bit and advances past it.
fn read_value(stream: &EncodedBitmap, at: &mut usize) -> bool {
    let value = stream.take(*at, VALUE_WIDTH) != 0;
    *at += VALUE_WIDTH;
    value
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

/// What every complex tile's own body holds, node by node: the
/// resolution tiles its flat runs say outright (unmasked), and the
/// nodes it masks out, by what they are -- a placed `Bound` tile bigger
/// than the resolution, a `Copied` tile, or finer content below the
/// resolution (read by the plain tree, or, at a 2x2 resolution, left as
/// a hole). A masked node is counted once, whatever its size.
#[derive(Default, Clone, Copy)]
struct ComplexCounts {
    complex_tiles: usize,
    complex_tiles_masking: usize,
    unmasked: usize,
    masked_bound: usize,
    masked_copied: usize,
    masked_finer: usize,
}

impl ComplexCounts {
    fn masked(&self) -> usize {
        self.masked_bound + self.masked_copied + self.masked_finer
    }

    fn add(&mut self, other: &ComplexCounts) {
        self.complex_tiles += other.complex_tiles;
        self.complex_tiles_masking += other.complex_tiles_masking;
        self.unmasked += other.unmasked;
        self.masked_bound += other.masked_bound;
        self.masked_copied += other.masked_copied;
        self.masked_finer += other.masked_finer;
    }

    /// Walks the top-level tree, down into every complex tile it finds.
    fn count(&mut self, node: &Node, region: Region) {
        match node {
            Node::Complex { depth, body } => {
                self.complex_tiles += 1;
                let before = self.masked();
                self.count_body(body, region, region.level + depth);
                if self.masked() > before {
                    self.complex_tiles_masking += 1;
                }
            }
            Node::Split(children) => {
                for (child_region, child) in region.children().into_iter().zip(children.iter()) {
                    self.count(child, child_region);
                }
            }
            _ => {}
        }
    }

    fn count_body(&mut self, node: &Node, region: Region, limit_level: usize) {
        let above_resolution = region.level < limit_level;
        match node {
            Node::Flat(values) => self.unmasked += values.len(),
            Node::Bound(_) if above_resolution => self.masked_bound += 1,
            Node::Bound(_) => self.unmasked += 1,
            Node::Copied { .. } => self.masked_copied += 1,
            Node::Hole => self.masked_finer += 1,
            Node::Split(_) if !above_resolution => self.masked_finer += 1,
            Node::Split(children) => {
                for (child_region, child) in region.children().into_iter().zip(children.iter()) {
                    self.count_body(child, child_region, limit_level);
                }
            }
            Node::Complex { .. } => unreachable!("a complex tile can never nest inside another's body"),
        }
    }
}

/// `part` as a percentage of `whole`, `0` when `whole` is.
fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        100.0 * part as f64 / whole as f64
    }
}

/// Bits against dsrn's own, in the same currency [`crate::dsrn_exp::tile_stream`]
/// already compares in, and how much each of the two masks.
pub fn run() {
    use crate::dsrn::{encode as dsrn_encode, FourByFour, Knobs, Masking, Workspace};
    use crate::samples;

    let knobs = Knobs { masking: Masking::Anywhere, four_by_four: FourByFour::ItsOwnGrammar };
    let (mut pyramid, mut work, mut dsrn_out) =
        (Pyramid::new(), Workspace::new(), crate::dsrn::Encoded::default());

    for (family, maps) in samples::every_family() {
        let (mut dsrn_bits, mut our_bits) = (0usize, 0usize);
        let (mut dsrn_nodes, mut dsrn_masked) = (0usize, 0usize);
        let mut complex = ComplexCounts::default();
        for bitmap in &maps {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            dsrn_encode(&pyramid, bitmap, knobs, &mut work, &mut dsrn_out);
            dsrn_bits += dsrn_out.bits();
            dsrn_nodes += dsrn_out.counts.nodes;
            dsrn_masked += dsrn_out.counts.masked_nodes;
            our_bits += encode(&pyramid, bitmap).len();

            let mut this_one = ComplexCounts::default();
            this_one.count(&complex_tiler(&greedy_tiler(&pyramid, bitmap)), Region::whole_bitmap());
            complex.add(&this_one);
        }
        let n = maps.len();
        println!(
            "\n  {family}, {n} bitmaps: dsrn {} bits a bitmap, tile-or-subdivide {} ({:+.1}%)",
            dsrn_bits / n,
            our_bits / n,
            100.0 * (our_bits as f64 - dsrn_bits as f64) / dsrn_bits as f64
        );
        println!(
            "    dsrn: {} nodes a bitmap, {:.1}% masked",
            dsrn_nodes / n,
            percent(dsrn_masked, dsrn_nodes)
        );
        let body_nodes = complex.unmasked + complex.masked();
        println!(
            "    complex tiles: {} a bitmap, {:.1}% of them masking; body nodes {:.2}% unmasked, {:.2}% masked \
             (bound {:.2}%, copied {:.2}%, finer {:.2}%)",
            complex.complex_tiles / n,
            percent(complex.complex_tiles_masking, complex.complex_tiles),
            percent(complex.unmasked, body_nodes),
            percent(complex.masked(), body_nodes),
            percent(complex.masked_bound, body_nodes),
            percent(complex.masked_copied, body_nodes),
            percent(complex.masked_finer, body_nodes),
        );
    }
}

/// Test-only: whether walking `tree` with [`claim_owners`], filling
/// whatever it leaves uncovered straight from `bitmap` instead of a
/// trailing raw pass, and [`resolve`]-ing the result reproduces
/// `bitmap` exactly -- checks `complex_tiler`'s own tree-building
/// independent of any bitstream at all.
#[cfg(test)]
pub(crate) fn tree_reproduces(tree: &Node, bitmap: &Bitmap) -> bool {
    let mut owner: Vec<Option<Owner>> = vec![None; 256 * 256];
    let mut covered = Bitmap::new();
    claim_owners(tree, Region::whole_bitmap(), Context::Open, &mut owner, &mut covered);
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if !covered.get(x, y) {
                owner[y as usize * 256 + x as usize] = Some(Owner::Bound(bitmap.get(x, y)));
            }
        }
    }
    let resolved = resolve(&owner);
    (0..=u8::MAX).all(|y| (0..=u8::MAX).all(|x| resolved.get(x, y) == bitmap.get(x, y)))
}

/// Test-only: writes the whole tree, from its own top.
#[cfg(test)]
pub(crate) fn write_whole_tree(tree: &Node) -> EncodedBitmap {
    let mut out = EncodedBitmap::default();
    write_node(tree, Region::whole_bitmap(), Context::Open, &mut out);
    out
}

/// Test-only: reads the whole tree back, mirroring [`write_whole_tree`].
#[cfg(test)]
pub(crate) fn parse_whole_tree(stream: &EncodedBitmap) -> Node {
    let mut at = 0usize;
    parse_node(stream, &mut at, Region::whole_bitmap(), Context::Open)
}
