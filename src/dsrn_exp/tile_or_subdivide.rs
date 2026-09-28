//! Walking the tree the greedy complex tiler already decided, to write
//! or read a bitstream -- nothing here ever decides anything. Building
//! that tree is entirely `greedy_tiles.rs`'s own job (`greedy_tiler`
//! then `complex_tiler`, producing one whole-bitmap [`Node`]); this
//! file only says how a [`Node`] is spelled out in bits, and how to read
//! that back -- "build trees first, then traverse the tree to encode,"
//! never combined. The residual pass (raw bits for whatever cells the
//! tree leaves uncovered) is a separate stage after the tree, in both
//! directions.
//!
//! # The grammar
//!
//! The whole plane is tiled with complex tiles: a bind is always a
//! complex tile, and a complex tile whose resolution is its own size is
//! just a *tile*. A node inside a complex tile is either related to it
//! (unmasked) or not (masked); a masked node may still be related to a
//! complex tile further out, nearest first.
//!
//! ```text
//! Every node starts with its relation bits: one for each complex tile
//! enclosing it that could relate it (the node covers whole tiles of
//! that complex tile's resolution), nearest first --
//!   0: related to this one -- nothing more here; its values come in
//!      that complex tile's payload
//!   1: not related -- ask the next one out
//! A node related to none of them goes on:
//!
//! One level above cells (2x2):
//! 1: a tile + 1 value bit
//! 0: a hole -- its four cells are left to the residual pass
//!
//! Any coarser level:
//! 1: leaf
//!    0: copy  + 1 far/near bit + 2 direction bits
//!    1: complex tile -- resolution_width(level) bits: depth, 0 meaning
//!       a tile, then
//!         depth 0 or 1: nothing (never masks)
//!         depth > 1:    0: no masking | 1: masking -- four child nodes
//!                       follow, this complex tile now the nearest
//!                       enclosing one
//!       then its payload: one value bit for every tile of its
//!       resolution related to it, in the order the body related them
//! 0: subdivide -- four child nodes
//! ```
//!
//! [`claim_owners`] is the one function that says which cells the tree
//! covers, shared by both directions so the residual pass's reading
//! order can never disagree between them.
//!
//! Decoding is separate steps, never interleaved: [`parse_node`] reads
//! the whole tree (each complex tile's payload filled in right after its
//! body is read); [`claim_owners`] records every cell as a value or a
//! direction to read one from; [`read_residual`] fills whatever is left;
//! [`resolve`] resolves copies by repeated sweeps, deferring a copy
//! whenever its source is not resolved yet. No cycle is possible -- a
//! copy always names something reading order puts before it -- so this
//! always finishes, backed by an assertion rather than blind trust.
//! Decoder speed is not a goal; simplicity is.

use crate::dsrn::region::{deepest_depth, Region, DIRECTIONS};
use crate::dsrn::stream::EncodedBitmap;
use crate::dsrn_exp::greedy_tiles::{complex_tiler, greedy_tiler, Node};
use crate::pyramid::{Pyramid, CELL_LEVEL};
use crate::Bitmap;

const LEAF_WIDTH: usize = 1;
const LEAF: u64 = 1;
const SUBDIVIDE: u64 = 0;
const HOLE: u64 = 0;

const CODE_WIDTH: usize = 1;
const COPY: u64 = 0;
const BIND: u64 = 1;

const FAR_WIDTH: usize = 1;
const DIRECTION_WIDTH: usize = 2;
const VALUE_WIDTH: usize = 1;

/// One bit for each enclosing complex tile that could relate a node:
/// related (unmasked) or not (masked).
const RELATION_WIDTH: usize = 1;
const RELATED: u64 = 0;
const UNRELATED: u64 = 1;

/// Whether a complex tile deeper than 1 masks anything at all -- `0`
/// goes straight to its payload, every tile of its resolution related
/// to it. Skipped at depth 0 and 1, which never mask.
const MASK_PRESENT_WIDTH: usize = 1;
const NO_MASKING: u64 = 0;
const MASKING: u64 = 1;

/// How many bits it takes to name a complex tile's depth at
/// `region.level`: `0` (a tile) up to `deepest_depth - 1` (a 2x2
/// resolution; 1x1 is never one, 1x1 tiles are always the residual
/// pass's own) -- `deepest_depth` values. A region's own level is
/// already known from its place in the tree -- free context, not a bit
/// anyone has to spend.
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
/// then the residual pass.
pub fn encode(pyramid: &Pyramid, bitmap: &Bitmap) -> EncodedBitmap {
    let tree = complex_tiler(&greedy_tiler(pyramid, bitmap));
    let mut out = EncodedBitmap::default();
    write_node(&tree, Region::whole_bitmap(), &mut Vec::new(), &mut out);

    let mut owner: Vec<Option<Owner>> = vec![None; 256 * 256];
    let mut covered = Bitmap::new();
    claim_owners(&tree, Region::whole_bitmap(), &mut Vec::new(), &mut owner, &mut covered);
    write_residual(&covered, bitmap, &mut out);
    out
}

/// Writes one [`Node`], for `region`. `enclosing` holds the resolutions
/// of the complex tiles enclosing it, outermost first.
fn write_node(node: &Node, region: Region, enclosing: &mut Vec<usize>, out: &mut EncodedBitmap) {
    for nesting in (0..enclosing.len()).rev() {
        if region.level > enclosing[nesting] {
            continue; // finer than that complex tile's resolution: it cannot relate this
        }
        if matches!(node, Node::Related { nesting: related, .. } if *related == nesting) {
            out.push_value(RELATED, RELATION_WIDTH);
            return;
        }
        out.push_value(UNRELATED, RELATION_WIDTH);
    }

    if region.level == CELL_LEVEL - 1 {
        match node {
            Node::Complex { depth: 0, body } => {
                out.push_value(LEAF, LEAF_WIDTH);
                write_payload(body, enclosing.len(), out);
            }
            Node::Hole => out.push_value(HOLE, LEAF_WIDTH),
            _ => unreachable!("one level above cells is always a tile or a hole"),
        }
        return;
    }

    match node {
        Node::Copied { far, direction } => {
            out.push_value(LEAF, LEAF_WIDTH);
            out.push_value(COPY, CODE_WIDTH);
            out.push_value(*far as u64, FAR_WIDTH);
            out.push_value(*direction as u64, DIRECTION_WIDTH);
        }
        Node::Complex { depth, body } => {
            out.push_value(LEAF, LEAF_WIDTH);
            out.push_value(BIND, CODE_WIDTH);
            out.push_value(*depth as u64, resolution_width(region.level));
            let nesting = enclosing.len();
            if *depth > 1 {
                let masking = matches!(**body, Node::Split(_));
                out.push_value(if masking { MASKING } else { NO_MASKING }, MASK_PRESENT_WIDTH);
            }
            if let Node::Split(children) = &**body {
                enclosing.push(region.level + depth);
                for (child_region, child) in region.children().into_iter().zip(children.iter()) {
                    write_node(child, child_region, enclosing, out);
                }
                enclosing.pop();
            }
            write_payload(body, nesting, out);
        }
        Node::Split(children) => {
            out.push_value(SUBDIVIDE, LEAF_WIDTH);
            for (child_region, child) in region.children().into_iter().zip(children.iter()) {
                write_node(child, child_region, enclosing, out);
            }
        }
        Node::Related { .. } => unreachable!("a related node was already written by its relation bits"),
        Node::Hole => unreachable!("a hole only ever appears one level above cells"),
    }
}

/// Writes a complex tile's payload: the values of every node in its
/// body related to it (`nesting`), in the order the body walk meets
/// them -- including inside complex tiles nested in it.
fn write_payload(body: &Node, nesting: usize, out: &mut EncodedBitmap) {
    let mut values = Vec::new();
    collect_payload(body, nesting, &mut values);
    for value in values {
        out.push_value(value as u64, VALUE_WIDTH);
    }
}

fn collect_payload(node: &Node, nesting: usize, values: &mut Vec<bool>) {
    match node {
        Node::Related { nesting: related, values: these } if *related == nesting => values.extend(these),
        Node::Complex { body, .. } => collect_payload(body, nesting, values),
        Node::Split(children) => children.iter().for_each(|child| collect_payload(child, nesting, values)),
        _ => {}
    }
}

/// What a cell's owner says, once the tree and the residual pass have
/// both been read: its own value, or a direction and distance to read
/// the matching cell from.
#[derive(Clone, Copy)]
enum Owner {
    Bound(bool),
    Copied { direction: usize, side: usize, far: bool },
}

/// Walks an already-built [`Node`] tree, recording every cell it covers
/// as a value or a direction to read one from, and marking `covered`
/// to match. The one function both `encode` (to find which cells the
/// tree leaves for the residual pass) and `decode` (to actually resolve
/// them) use, so the two can never disagree about which cells those are.
fn claim_owners(
    node: &Node,
    region: Region,
    enclosing: &mut Vec<usize>,
    owner: &mut [Option<Owner>],
    covered: &mut Bitmap,
) {
    match node {
        Node::Related { nesting, values } => {
            let depth = enclosing[*nesting] - region.level;
            for (tile, &value) in region.tiles_at_depth(depth).into_iter().zip(values) {
                mark_owner(owner, tile, Owner::Bound(value));
            }
            mark_covered(covered, region);
        }
        Node::Copied { far, direction } => {
            mark_owner(owner, region, Owner::Copied { direction: *direction, side: region.side_in_cells(), far: *far });
            mark_covered(covered, region);
        }
        Node::Hole => {} // its cells are left entirely to the residual pass
        Node::Complex { depth, body } => {
            enclosing.push(region.level + depth);
            claim_owners(body, region, enclosing, owner, covered);
            enclosing.pop();
        }
        Node::Split(children) => {
            for (child_region, child) in region.children().into_iter().zip(children.iter()) {
                claim_owners(child, child_region, enclosing, owner, covered);
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

/// The residual pass: one raw bit for every cell the tree leaves
/// uncovered, in reading order, after the whole tree.
fn write_residual(covered: &Bitmap, bitmap: &Bitmap, out: &mut EncodedBitmap) {
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if !covered.get(x, y) {
                out.push_value(bitmap.get(x, y) as u64, VALUE_WIDTH);
            }
        }
    }
}

/// Reads back what [`write_residual`] wrote, into the cells the tree
/// left uncovered.
fn read_residual(covered: &Bitmap, stream: &EncodedBitmap, at: &mut usize, owner: &mut [Option<Owner>]) {
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if !covered.get(x, y) {
                owner[y as usize * 256 + x as usize] = Some(Owner::Bound(read_value(stream, at)));
            }
        }
    }
}

/// Decodes a stream written by [`encode`].
pub fn decode(stream: &EncodedBitmap) -> Bitmap {
    let mut at = 0usize;
    let tree = parse_node(stream, &mut at, Region::whole_bitmap(), &mut Vec::new());

    let mut owner: Vec<Option<Owner>> = vec![None; 256 * 256];
    let mut covered = Bitmap::new();
    claim_owners(&tree, Region::whole_bitmap(), &mut Vec::new(), &mut owner, &mut covered);
    read_residual(&covered, stream, &mut at, &mut owner);
    resolve(&owner)
}

/// Reads one [`Node`], for `region`, mirroring [`write_node`] exactly.
fn parse_node(stream: &EncodedBitmap, at: &mut usize, region: Region, enclosing: &mut Vec<usize>) -> Node {
    for nesting in (0..enclosing.len()).rev() {
        if region.level > enclosing[nesting] {
            continue;
        }
        if read_bits(stream, at, RELATION_WIDTH) == RELATED {
            // Placeholders: the values come in that complex tile's
            // payload, read once its whole body has been.
            let count = 1usize << (2 * (enclosing[nesting] - region.level));
            return Node::Related { nesting, values: vec![false; count] };
        }
    }

    let leaf = read_bits(stream, at, LEAF_WIDTH) == LEAF;
    if region.level == CELL_LEVEL - 1 {
        if !leaf {
            return Node::Hole;
        }
        return Node::Complex { depth: 0, body: Box::new(parse_complex_body(stream, at, region, 0, enclosing)) };
    }
    if !leaf {
        return Node::Split(Box::new(region.children().map(|child| parse_node(stream, at, child, enclosing))));
    }
    if read_bits(stream, at, CODE_WIDTH) == COPY {
        let far = read_bits(stream, at, FAR_WIDTH) != 0;
        let direction = read_bits(stream, at, DIRECTION_WIDTH) as usize;
        return Node::Copied { far, direction };
    }
    let depth = read_bits(stream, at, resolution_width(region.level)) as usize;
    Node::Complex { depth, body: Box::new(parse_complex_body(stream, at, region, depth, enclosing)) }
}

/// Reads a complex tile's body and then its payload, mirroring the
/// `Complex` arm of [`write_node`].
fn parse_complex_body(
    stream: &EncodedBitmap,
    at: &mut usize,
    region: Region,
    depth: usize,
    enclosing: &mut Vec<usize>,
) -> Node {
    let nesting = enclosing.len();
    let masking = depth > 1 && read_bits(stream, at, MASK_PRESENT_WIDTH) == MASKING;
    let mut body = if masking {
        enclosing.push(region.level + depth);
        let children = region.children().map(|child| parse_node(stream, at, child, enclosing));
        enclosing.pop();
        Node::Split(Box::new(children))
    } else {
        Node::Related { nesting, values: vec![false; 1usize << (2 * depth)] }
    };
    read_payload(&mut body, nesting, stream, at);
    body
}

/// Fills in the values of every node related to `nesting`, in the same
/// order [`collect_payload`] wrote them.
fn read_payload(node: &mut Node, nesting: usize, stream: &EncodedBitmap, at: &mut usize) {
    match node {
        Node::Related { nesting: related, values } if *related == nesting => {
            for value in values.iter_mut() {
                *value = read_value(stream, at);
            }
        }
        Node::Complex { body, .. } => read_payload(body, nesting, stream, at),
        Node::Split(children) => children.iter_mut().for_each(|child| read_payload(child, nesting, stream, at)),
        _ => {}
    }
}

/// Reads `width` bits and advances past them.
fn read_bits(stream: &EncodedBitmap, at: &mut usize, width: usize) -> u64 {
    let bits = stream.take(*at, width);
    *at += width;
    bits
}

/// Reads one value bit and advances past it.
fn read_value(stream: &EncodedBitmap, at: &mut usize) -> bool {
    read_bits(stream, at, VALUE_WIDTH) != 0
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

/// What the tree holds: tiles, complex tiles by how deeply nested they
/// are (`0` = enclosed by none), and, inside every complex tile's body,
/// its nodes -- related to it (unmasked, one per tile of its
/// resolution), or masked, by what they are instead. A masked node is
/// counted once, whatever its size, and belongs to the complex tile
/// whose body directly holds it.
#[derive(Default, Clone)]
struct ComplexCounts {
    tiles: usize,
    complex_tiles_at_nesting: Vec<usize>,
    complex_tiles_masking: usize,
    unmasked: usize,
    masked_related_further_out: usize,
    masked_copied: usize,
    masked_tile: usize,
    masked_nested: usize,
    masked_hole: usize,
}

impl ComplexCounts {
    fn masked(&self) -> usize {
        self.masked_related_further_out + self.masked_copied + self.masked_tile + self.masked_nested + self.masked_hole
    }

    fn complex_tiles(&self) -> usize {
        self.complex_tiles_at_nesting.iter().sum()
    }

    fn add(&mut self, other: &ComplexCounts) {
        self.tiles += other.tiles;
        if self.complex_tiles_at_nesting.len() < other.complex_tiles_at_nesting.len() {
            self.complex_tiles_at_nesting.resize(other.complex_tiles_at_nesting.len(), 0);
        }
        for (total, added) in self.complex_tiles_at_nesting.iter_mut().zip(&other.complex_tiles_at_nesting) {
            *total += added;
        }
        self.complex_tiles_masking += other.complex_tiles_masking;
        self.unmasked += other.unmasked;
        self.masked_related_further_out += other.masked_related_further_out;
        self.masked_copied += other.masked_copied;
        self.masked_tile += other.masked_tile;
        self.masked_nested += other.masked_nested;
        self.masked_hole += other.masked_hole;
    }

    /// `inside`: the nesting of the complex tile whose body directly
    /// holds `node`, if any.
    fn count(&mut self, node: &Node, inside: Option<usize>) {
        match node {
            Node::Related { nesting, values } => {
                if inside == Some(*nesting) {
                    self.unmasked += values.len();
                } else {
                    self.masked_related_further_out += 1;
                }
            }
            Node::Copied { .. } if inside.is_some() => self.masked_copied += 1,
            Node::Hole if inside.is_some() => self.masked_hole += 1,
            Node::Copied { .. } | Node::Hole => {}
            Node::Complex { depth: 0, .. } => {
                self.tiles += 1;
                if inside.is_some() {
                    self.masked_tile += 1;
                }
            }
            Node::Complex { body, .. } => {
                if inside.is_some() {
                    self.masked_nested += 1;
                }
                let nesting = inside.map_or(0, |outer| outer + 1);
                if self.complex_tiles_at_nesting.len() <= nesting {
                    self.complex_tiles_at_nesting.resize(nesting + 1, 0);
                }
                self.complex_tiles_at_nesting[nesting] += 1;
                if matches!(**body, Node::Split(_)) {
                    self.complex_tiles_masking += 1;
                }
                self.count(body, Some(nesting));
            }
            Node::Split(children) => children.iter().for_each(|child| self.count(child, inside)),
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
            this_one.count(&complex_tiler(&greedy_tiler(&pyramid, bitmap)), None);
            complex.add(&this_one);
        }
        let n = maps.len();
        let per_bitmap = |count: usize| count as f64 / n as f64;
        println!(
            "\n  {family}, {n} bitmaps: dsrn {} bits a bitmap, tile-or-subdivide {} ({:+.1}%)",
            dsrn_bits / n,
            our_bits / n,
            100.0 * (our_bits as f64 - dsrn_bits as f64) / dsrn_bits as f64
        );
        println!("    dsrn: {} nodes a bitmap, {:.1}% masked", dsrn_nodes / n, percent(dsrn_masked, dsrn_nodes));
        let by_nesting: Vec<String> =
            complex.complex_tiles_at_nesting.iter().map(|&count| format!("{:.1}", per_bitmap(count))).collect();
        println!(
            "    complex tiles a bitmap, by nesting: [{}], {:.1}% of them masking; tiles a bitmap: {:.1}",
            by_nesting.join(", "),
            percent(complex.complex_tiles_masking, complex.complex_tiles()),
            per_bitmap(complex.tiles),
        );
        let body_nodes = complex.unmasked + complex.masked();
        println!(
            "    complex tile body nodes: {:.2}% unmasked, {:.2}% masked (related further out {:.2}%, copied {:.2}%, \
             tile {:.2}%, nested complex tile {:.2}%, hole {:.2}%)",
            percent(complex.unmasked, body_nodes),
            percent(complex.masked(), body_nodes),
            percent(complex.masked_related_further_out, body_nodes),
            percent(complex.masked_copied, body_nodes),
            percent(complex.masked_tile, body_nodes),
            percent(complex.masked_nested, body_nodes),
            percent(complex.masked_hole, body_nodes),
        );
    }
}

/// Test-only: whether walking `tree` with [`claim_owners`], filling
/// whatever it leaves uncovered straight from `bitmap` instead of a
/// residual pass, and [`resolve`]-ing the result reproduces `bitmap`
/// exactly -- checks `complex_tiler`'s own tree-building independent of
/// any bitstream at all.
#[cfg(test)]
pub(crate) fn tree_reproduces(tree: &Node, bitmap: &Bitmap) -> bool {
    let mut owner: Vec<Option<Owner>> = vec![None; 256 * 256];
    let mut covered = Bitmap::new();
    claim_owners(tree, Region::whole_bitmap(), &mut Vec::new(), &mut owner, &mut covered);
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
    write_node(tree, Region::whole_bitmap(), &mut Vec::new(), &mut out);
    out
}

/// Test-only: reads the whole tree back, mirroring [`write_whole_tree`].
#[cfg(test)]
pub(crate) fn parse_whole_tree(stream: &EncodedBitmap) -> Node {
    let mut at = 0usize;
    parse_node(stream, &mut at, Region::whole_bitmap(), &mut Vec::new())
}
