//! One descent, one code table: tile size, binding and copying decided
//! region by region.
//!
//! A **region** is a node of the quadtree. A **tile** is an aligned
//! square of an aligned size; it is not a region, and nothing in the
//! tree corresponds to one. What the encoding takes advantage of is
//! that the two line up: a region's tiles at any size are exactly the
//! regions some number of levels below it, so the pyramid answers a
//! question about tiles by looking at regions, and a tile that has to
//! be described separately is already a region that can describe
//! itself.
//!
//! # The codes
//!
//! Every region writes two bits, and the decoder always knows which
//! region it is standing on, so nothing has to say where it is or how
//! big it is.
//!
//! | code | meaning | what follows |
//! |------|---------|--------------|
//! | `00` | split | the four children, in reading order |
//! | `01` | bind | a tile size, then one payload bit per tile |
//! | `10` | copy | a direction |
//! | `11` | masked copy | a four bit quadrant mask and a direction |
//!
//! # Binding, and what a binding leaves out
//!
//! A binding names its tile size as a depth below the region, written
//! in unary, and then says whether it is whole. A whole binding has a
//! payload bit for every tile and that is all of it.
//!
//! A binding that is not whole carries a four bit quadrant mask: it
//! binds the quadrants the mask covers, and the quadrants it does not
//! follow as regions of their own, nested inside the binding. Those
//! may mask in turn, so the masking reaches any depth at four bits a
//! level rather than one bit a tile.
//!
//! That is what makes a region's payload leave out the bits a nested
//! region would have needed: a wide plain area with an awkward corner
//! binds the other three quadrants and lets the corner describe
//! itself.
//!
//! # Which tile size
//!
//! The one that fully covers the largest homogeneous region inside
//! this one. Coarser than that and the largest plain thing there is
//! still lands inside a heterogeneous tile, which describes nothing;
//! finer and it is split into four payload bits where one would have
//! done.
//!
//! It folds upward in one number: a homogeneous region is one tile at
//! depth nought, and any other region is one level finer than its
//! *shallowest* child.
//!
//! A quadrant can only be bound if every one of its tiles is
//! homogeneous, which is the same fold with a max where that one has
//! a min. The quadrants that fail it are the ones that nest.

use crate::dsrn::region::{alike, copy_in, row_span, Region, CHILDREN, DIRECTIONS};
use crate::dsrn::stream::Bits;
use crate::dsrn::{Pyramid, LEVELS};
use crate::BitMatrix;

/// The two bit code every region writes.
const SPLIT: u64 = 0b00;
const BIND: u64 = 0b01;
const COPY: u64 = 0b10;
const MASKED_COPY: u64 = 0b11;

/// Whether a binding covers all of its region, or leaves nested
/// regions out of its payload.
const WHOLE: u64 = 0;
const NESTED: u64 = 1;

/// A quadrant mask with every quadrant covered, which is the whole
/// region and needs no mask to say so.
const EVERY_QUADRANT: u64 = 0b1111;

/// The widths the costs are counted in.
const CODE: usize = 2;
const DIRECTION: usize = 2;
const QUADRANTS: usize = 4;
const NESTING: usize = 1;

/// A tile size written in unary: `depth` ones and a nought.
const fn unary(depth: usize) -> usize {
    depth + 1
}

/// Tiles in a region whose tiles are `depth` levels below it.
const fn tiles(depth: usize) -> usize {
    1 << (2 * depth)
}

/// What a homogeneous region costs: the code, a depth of nought, a
/// whole binding, and the one bit that says what it holds.
const UNIT: u32 = (CODE + unary(0) + NESTING + 1) as u32;

/// How a region picks its tile size. Splitting, copying and masked
/// copying compete under either of them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Choosing {
    /// The rule: the size that fully covers the largest homogeneous
    /// region inside this one.
    LargestHomogeneousTile,
    /// The size whose binding costs least.
    CheapestTileSize,
    /// The coarsest size that covers the whole region: every tile of
    /// every quadrant homogeneous, so nothing nests.
    CoversTheWholeRegion,
}

impl Choosing {
    pub const ALL: [Choosing; 3] = [
        Choosing::LargestHomogeneousTile,
        Choosing::CheapestTileSize,
        Choosing::CoversTheWholeRegion,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Choosing::LargestHomogeneousTile => {
                "the size that covers the largest homogeneous region inside"
            }
            Choosing::CheapestTileSize => "the size whose binding costs least",
            Choosing::CoversTheWholeRegion => "the coarsest size that covers the whole region",
        }
    }
}

/// How many of each code an encode wrote.
#[derive(Default, Clone, Copy)]
pub struct Counts {
    pub splits: usize,
    pub whole_bindings: usize,
    pub nested_bindings: usize,
    pub copies: usize,
    pub masked_copies: usize,
    /// Quadrants a binding left to a region of their own.
    pub nested_quadrants: usize,
}

/// What an encode produces.
#[derive(Default)]
pub struct Encoded {
    pub counts: Counts,
    /// The codes, tile sizes, nesting masks, quadrant masks and
    /// directions.
    pub tree: Bits,
    /// One bit per tile a binding described, in reading order.
    pub payload: Bits,
}

impl Encoded {
    /// Every bit of the encoding.
    pub fn bits(&self) -> usize {
        self.tree.len() + self.payload.len()
    }

    fn clear(&mut self) {
        self.counts = Counts::default();
        self.tree.clear();
        self.payload.clear();
    }
}

/// Which regions the decoder will already hold, and so may be copied
/// from.
///
/// Everything an encode binds is a whole region, never part of one,
/// so this is a bit per region rather than a bit per cell. That turns
/// "does the decoder hold this region yet" from a scan across its
/// cells into a walk up its ancestors.
///
/// Two ways a region can be held without its own bit being set, and
/// both are kept: an ancestor was bound whole, which the walk upwards
/// finds, and all four children were bound separately, which
/// [`Bound::bind`] folds upwards as it goes.
struct Bound {
    plane: [Box<[u64]>; LEVELS + 1],
}

impl Bound {
    fn new() -> Self {
        Self {
            plane: std::array::from_fn(|level| {
                let across = Pyramid::side(level);
                vec![0u64; (across * across).div_ceil(64)].into_boxed_slice()
            }),
        }
    }

    fn clear(&mut self) {
        for plane in &mut self.plane {
            plane.fill(0);
        }
    }

    fn bit(level: usize, x: usize, y: usize) -> (usize, usize) {
        let at = y * Pyramid::side(level) + x;
        (at / 64, at % 64)
    }

    fn get(&self, level: usize, x: usize, y: usize) -> bool {
        let (word, shift) = Self::bit(level, x, y);
        self.plane[level][word] >> shift & 1 != 0
    }

    /// Binds a whole region, and every ancestor that this completes.
    fn bind(&mut self, region: Region) {
        let (mut level, mut x, mut y) = (region.level, region.x, region.y);
        loop {
            let (word, shift) = Self::bit(level, x, y);
            self.plane[level][word] |= 1 << shift;
            if level == LEVELS {
                return;
            }
            let (px, py) = (x / 2, y / 2);
            if !CHILDREN.iter().all(|&(dx, dy)| self.get(level, px * 2 + dx, py * 2 + dy)) {
                return;
            }
            (level, x, y) = (level + 1, px, py);
        }
    }

    /// Whether the decoder holds a region: it, or any ancestor of it.
    /// Outside the bitmap is never held.
    fn has(&self, level: usize, x: isize, y: isize) -> bool {
        let across = Pyramid::side(level) as isize;
        if x < 0 || y < 0 || x >= across || y >= across {
            return false;
        }
        let (mut level, mut x, mut y) = (level, x as usize, y as usize);
        loop {
            if self.get(level, x, y) {
                return true;
            }
            if level == LEVELS {
                return false;
            }
            (level, x, y) = (level + 1, x / 2, y / 2);
        }
    }
}

/// One direction's four bits of a copy mask.
fn quadrants(mask: u16, dir: usize) -> u64 {
    (mask >> (CHILDREN.len() * dir)) as u64 & EVERY_QUADRANT
}

/// The room an encode works in, found once and reused.
pub struct Workspace {
    /// Per region, the tile size it settled on, as a depth below the
    /// region. Nought means the region is homogeneous and is one
    /// tile.
    tile_size: [Box<[u8]>; LEVELS + 1],
    /// Per region, the coarsest size at which *every* tile of it is
    /// homogeneous, as a depth below the region. A region can only be
    /// bound by its parent when the parent's tile size is at least
    /// this fine.
    every: [Box<[u8]>; LEVELS + 1],
    /// Per region, the cheapest description of it.
    cost: [Box<[u32]>; LEVELS + 1],
    bound: Bound,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    /// A workspace with its room already found.
    pub fn new() -> Self {
        let regions = |level: usize| Pyramid::side(level) * Pyramid::side(level);
        let depths =
            || std::array::from_fn(|level: usize| vec![0u8; regions(level.max(1))].into_boxed_slice());
        Self {
            tile_size: depths(),
            every: depths(),
            cost: std::array::from_fn(|level| {
                vec![0u32; regions(level.max(1))].into_boxed_slice()
            }),
            bound: Bound::new(),
        }
    }

    /// Where a region sits in its level's run.
    fn at(level: usize, x: usize, y: usize) -> usize {
        y * Pyramid::side(level) + x
    }

    /// The tile size a region settled on, as [`minimal_tile_sizes`]
    /// left it.
    fn tile_size_of(&self, region: Region) -> usize {
        if region.level == 0 {
            return 0;
        }
        self.tile_size[region.level][Self::at(region.level, region.x, region.y)] as usize
    }

    /// The coarsest size at which every tile of a region is
    /// homogeneous, as [`minimal_tile_sizes`] left it.
    fn every_of(&self, region: Region) -> usize {
        if region.level == 0 {
            return 0;
        }
        self.every[region.level][Self::at(region.level, region.x, region.y)] as usize
    }

    /// The cheapest description of a region, as
    /// [`minimal_tile_sizes`] left it.
    fn cost_of(&self, region: Region) -> u32 {
        if region.level == 0 {
            return UNIT;
        }
        self.cost[region.level][Self::at(region.level, region.x, region.y)]
    }
}

/// The four children of a region, in reading order.
fn children_of(region: Region) -> [Region; 4] {
    CHILDREN.map(|(dx, dy)| Region {
        level: region.level - 1,
        x: region.x * 2 + dx,
        y: region.y * 2 + dy,
    })
}

/// Whether two regions of the same size hold the same cells.
///
/// The pyramid answers it outright whenever either is homogeneous:
/// two homogeneous regions agree exactly when they hold the same
/// thing, and a homogeneous region never equals a heterogeneous one.
/// Only two heterogeneous regions have to be read.
fn matches(
    pyramid: &Pyramid,
    bits: &BitMatrix,
    level: usize,
    a: (usize, usize),
    b: (usize, usize),
) -> bool {
    if level == 0 {
        return bits.get(a.0 as u8, a.1 as u8) == bits.get(b.0 as u8, b.1 as u8);
    }
    match (pyramid.at(level, a.0, a.1), pyramid.at(level, b.0, b.1)) {
        (None, None) => alike(bits, level, a, (b.0 as isize, b.1 as isize)),
        (a, b) => a == b,
    }
}

/// Whether a region is the same as one of the neighbours it could
/// copy from.
///
/// [`minimal_tile_sizes`] asks this of every heterogeneous region and
/// [`copy_mask`] only of the few the descent reaches, so this is the
/// one that has to be cheap: four compares that each stop at the
/// first word that differs, rather than sixteen.
fn could_copy(pyramid: &Pyramid, bits: &BitMatrix, region: Region) -> bool {
    let across = Pyramid::side(region.level) as isize;
    DIRECTIONS.iter().any(|&(dx, dy)| {
        let (nx, ny) = (region.x as isize + dx, region.y as isize + dy);
        nx >= 0
            && ny >= 0
            && nx < across
            && ny < across
            && matches(pyramid, bits, region.level, (region.x, region.y), (nx as usize, ny as usize))
    })
}

/// Which of a region's four quadrants match the four of the
/// neighbour in each direction, four bits to a direction.
///
/// The offsets are the region's own step measured in quadrants, so
/// this answers the region's question and not its quadrants': the
/// region matches its neighbour when all four bits are set, and when
/// only some are, those are the quadrants a masked copy covers.
fn copy_mask(pyramid: &Pyramid, bits: &BitMatrix, region: Region) -> u16 {
    let children = children_of(region);
    let across = Pyramid::side(region.level - 1) as isize;
    let mut mask = 0u16;
    for (dir, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
        for (bit, child) in children.iter().enumerate() {
            let (nx, ny) = (child.x as isize + 2 * dx, child.y as isize + 2 * dy);
            if nx >= 0
                && ny >= 0
                && nx < across
                && ny < across
                && matches(
                    pyramid,
                    bits,
                    child.level,
                    (child.x, child.y),
                    (nx as usize, ny as usize),
                )
            {
                mask |= 1 << (CHILDREN.len() * dir + bit);
            }
        }
    }
    mask
}

/// Which quadrants of a region a binding at a tile size can cover:
/// the ones whose every tile is homogeneous at that size.
fn bindable(work: &Workspace, region: Region, depth: usize) -> u64 {
    let mut mask = 0;
    for (bit, child) in children_of(region).into_iter().enumerate() {
        if work.every_of(child) <= depth - 1 {
            mask |= 1 << bit;
        }
    }
    mask
}

/// What a binding at a tile size costs: the code, the size, whether
/// it is whole, a quadrant mask if it is not, a payload bit per tile
/// it covers, and a description of each quadrant it does not.
fn binding_cost(work: &Workspace, region: Region, depth: usize, mask: u64) -> usize {
    if depth == 0 {
        return UNIT as usize;
    }
    let children = children_of(region);
    let mut cost = CODE + unary(depth) + NESTING;
    if mask != EVERY_QUADRANT {
        cost += QUADRANTS;
    }
    for (bit, &child) in children.iter().enumerate() {
        cost += if mask >> bit & 1 == 1 {
            tiles(depth - 1)
        } else {
            work.cost_of(child) as usize
        };
    }
    cost
}

/// Reads the pyramid bottom up, leaving every region its tile size
/// and what it costs to describe.
///
/// It stops at a homogeneous region rather than walking through it:
/// that region costs [`UNIT`] whatever is beneath it, and the descent
/// never asks about anything beneath it. Because a region that stops
/// is never descended into, its children keep whatever the last
/// bitmap left in them, and nothing may read those.
fn minimal_tile_sizes(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bits: &BitMatrix,
    region: Region,
    choosing: Choosing,
) -> (u8, u8, u32) {
    // A cell is homogeneous and has nothing under it. The pyramid
    // does not hold the cells -- the bitmap already is them -- so it
    // is never asked about one.
    if region.level == 0 {
        return (0, 0, UNIT);
    }

    let at = Workspace::at(region.level, region.x, region.y);
    if pyramid.at(region.level, region.x, region.y).is_some() {
        work.tile_size[region.level][at] = 0;
        work.every[region.level][at] = 0;
        work.cost[region.level][at] = UNIT;
        return (0, 0, UNIT);
    }

    // The largest homogeneous region inside this one is one level
    // below the shallowest of its children's; the size at which every
    // tile is homogeneous is one below the deepest.
    let (mut shallowest, mut deepest, mut split) = (u8::MAX, 0u8, CODE as u32);
    for child in children_of(region) {
        let (largest, every, cost) = minimal_tile_sizes(work, pyramid, bits, child, choosing);
        shallowest = shallowest.min(largest);
        deepest = deepest.max(every);
        split += cost;
    }
    let (largest, every) = (shallowest + 1, deepest + 1);
    work.every[region.level][at] = every;

    // Settled here rather than during the descent, which would ask
    // the same question of the same region a second time.
    let depth = match choosing {
        Choosing::LargestHomogeneousTile => largest as usize,
        Choosing::CheapestTileSize => (1..=region.level)
            .min_by_key(|&size| binding_cost(work, region, size, bindable(work, region, size)))
            .unwrap_or(1),
        Choosing::CoversTheWholeRegion => every as usize,
    };
    work.tile_size[region.level][at] = depth as u8;
    let mut cost =
        binding_cost(work, region, depth, bindable(work, region, depth)).min(split as usize) as u32;

    // Which neighbours the decoder will hold depends on what every
    // region above this one chose, which this runs before, so a copy
    // is priced at what it would cost if the neighbour were there.
    // That only ever makes a region look cheaper than it is, so a
    // region that splits on the strength of it still comes back
    // whole; it just spends a few bits more than this promised.
    let copy = (CODE + DIRECTION) as u32;
    if cost > copy && could_copy(pyramid, bits, region) {
        cost = copy;
    }

    work.cost[region.level][at] = cost;
    (largest, every, cost)
}

/// The tiles of a region at a tile size, as regions.
fn tiles_of(region: Region, depth: usize) -> impl Iterator<Item = Region> {
    let across = 1usize << depth;
    let (level, x, y) = (region.level - depth, region.x * across, region.y * across);
    (0..tiles(depth)).map(move |at| Region { level, x: x + at % across, y: y + at / across })
}

/// Writes a payload bit for every tile of a region, which is a run of
/// the pyramid's value plane, or of the bitmap where the tiles are
/// cells.
fn whole_payload_out(
    pyramid: &Pyramid,
    bits: &BitMatrix,
    region: Region,
    depth: usize,
    out: &mut Encoded,
) {
    let across = 1usize << depth;
    let level = region.level - depth;
    for row in region.y * across..(region.y + 1) * across {
        let mut done = 0;
        while done < across {
            let take = (across - done).min(64);
            let from = region.x * across + done;
            let word = if level == 0 {
                row_span(bits, row, from, take)
            } else {
                pyramid.held_span(level, row, from, take)
            };
            out.payload.push(word, take);
            done += take;
        }
    }
}

/// Writes a tile size in unary.
fn tile_size_out(depth: usize, out: &mut Encoded) {
    if depth > 0 {
        out.tree.push((1u64 << depth) - 1, depth);
    }
    out.tree.push(0, 1);
}

/// Encodes the bitmap. The pyramid must already hold it.
pub fn encode(
    pyramid: &Pyramid,
    bits: &BitMatrix,
    choosing: Choosing,
    work: &mut Workspace,
    out: &mut Encoded,
) {
    out.clear();
    work.bound.clear();
    let whole = Region { level: LEVELS, x: 0, y: 0 };
    minimal_tile_sizes(work, pyramid, bits, whole, choosing);
    describe(work, pyramid, bits, whole, out);
}

/// What the descent settled on for a region.
#[derive(Clone, Copy)]
enum Chosen {
    /// Bound at a tile size, over the quadrants the mask covers.
    Bind(usize, u64),
    /// Copied whole from a direction.
    Copy(usize),
    /// Copied over the quadrants the mask covers, the rest left to
    /// those children.
    MaskedCopy(u64, usize),
    /// Left to the four children.
    Split,
}

/// Describes one region, and whatever its description leaves out.
fn describe(
    work: &mut Workspace,
    pyramid: &Pyramid,
    bits: &BitMatrix,
    region: Region,
    out: &mut Encoded,
) {
    let depth = work.tile_size_of(region);

    // A homogeneous region is done outright, and this is also what
    // keeps the descent out of the part of the tree the survey
    // pruned: it stopped here, so this region's children hold nothing
    // anyone may read.
    if depth == 0 {
        out.counts.whole_bindings += 1;
        out.tree.push(BIND, CODE);
        tile_size_out(0, out);
        out.tree.push(WHOLE, NESTING);
        whole_payload_out(pyramid, bits, region, 0, out);
        work.bound.bind(region);
        return;
    }

    let binds = bindable(work, region, depth);

    // Binding is tried first and wins ties: capturing homogeneous
    // area outright never costs a neighbour's luck, and leaves that
    // luck to a region with nothing else to spend.
    let (mut best, mut how) =
        (binding_cost(work, region, depth, binds), Chosen::Bind(depth, binds));

    let children = children_of(region);
    let outside = |work: &Workspace, mask: u64| -> usize {
        (0..CHILDREN.len())
            .filter(|bit| mask >> bit & 1 == 0)
            .map(|bit| work.cost_of(children[bit]) as usize)
            .sum()
    };

    let split = CODE + outside(work, 0);
    if split < best {
        (best, how) = (split, Chosen::Split);
    }

    // The survey already knows which quadrants match which neighbour.
    // All that is left is whether the decoder will hold them by the
    // time it arrives, which only the descent can say.
    let mask = copy_mask(pyramid, bits, region);
    for (dir, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
        let mut mask = quadrants(mask, dir);
        for (bit, child) in children.iter().enumerate() {
            let (nx, ny) = (child.x as isize + 2 * dx, child.y as isize + 2 * dy);
            if mask >> bit & 1 == 1 && !work.bound.has(child.level, nx, ny) {
                mask &= !(1 << bit);
            }
        }
        let cost = match mask {
            0 => continue,
            EVERY_QUADRANT => CODE + DIRECTION,
            _ => CODE + QUADRANTS + DIRECTION + outside(work, mask),
        };
        if cost < best {
            (best, how) = (
                cost,
                if mask == EVERY_QUADRANT {
                    Chosen::Copy(dir)
                } else {
                    Chosen::MaskedCopy(mask, dir)
                },
            );
        }
    }

    // Whatever a description covers is bound before anything it
    // leaves out is described, so a hole may read the quadrants
    // around it.
    match how {
        Chosen::Bind(depth, mask) => {
            out.tree.push(BIND, CODE);
            tile_size_out(depth, out);
            if mask == EVERY_QUADRANT {
                out.counts.whole_bindings += 1;
                out.tree.push(WHOLE, NESTING);
                whole_payload_out(pyramid, bits, region, depth, out);
                work.bound.bind(region);
                return;
            }
            out.counts.nested_bindings += 1;
            out.tree.push(NESTED, NESTING);
            out.tree.push(mask, QUADRANTS);
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 1 {
                    whole_payload_out(pyramid, bits, child, depth - 1, out);
                    work.bound.bind(child);
                }
            }
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 0 {
                    out.counts.nested_quadrants += 1;
                    describe(work, pyramid, bits, child, out);
                }
            }
        }
        Chosen::Copy(dir) => {
            out.counts.copies += 1;
            out.tree.push(COPY, CODE);
            out.tree.push(dir as u64, DIRECTION);
            work.bound.bind(region);
        }
        Chosen::Split => {
            out.counts.splits += 1;
            out.tree.push(SPLIT, CODE);
            for child in children {
                describe(work, pyramid, bits, child, out);
            }
        }
        Chosen::MaskedCopy(mask, dir) => {
            out.counts.masked_copies += 1;
            out.tree.push(MASKED_COPY, CODE);
            out.tree.push(mask, QUADRANTS);
            out.tree.push(dir as u64, DIRECTION);
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 1 {
                    work.bound.bind(child);
                }
            }
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 0 {
                    describe(work, pyramid, bits, child, out);
                }
            }
        }
    }
}

/// The decoder's place in the two streams.
#[derive(Default)]
struct Reading {
    tree: usize,
    payload: usize,
}

impl Reading {
    fn take(&mut self, out: &Encoded, width: usize) -> u64 {
        let got = out.tree.take(self.tree, width).unwrap_or(0);
        self.tree += width;
        got
    }

    /// A tile size, read back from unary.
    fn tile_size(&mut self, out: &Encoded) -> usize {
        let mut depth = 0;
        while self.take(out, 1) == 1 {
            depth += 1;
        }
        depth
    }

    fn value(&mut self, out: &Encoded) -> bool {
        let got = out.payload.take(self.payload, 1).unwrap_or(0) == 1;
        self.payload += 1;
        got
    }
}

/// Reads the bitmap back. The workspace need not be the one that
/// encoded it.
pub fn decode(out: &Encoded, bits: &mut BitMatrix) {
    bits.words.fill(0);
    let mut reading = Reading::default();
    undescribe(&mut reading, out, bits, Region { level: LEVELS, x: 0, y: 0 });
}

/// Fills in a whole tile of a region.
fn fill(bits: &mut BitMatrix, tile: Region) {
    let side = 1usize << tile.level;
    bits.set_rect(
        (tile.x * side) as i64,
        (tile.y * side) as i64,
        (tile.x * side + side - 1) as i64,
        (tile.y * side + side - 1) as i64,
    );
}

/// Puts back one region, and whatever its description left out.
fn undescribe(reading: &mut Reading, out: &Encoded, bits: &mut BitMatrix, region: Region) {
    match reading.take(out, CODE) {
        BIND => {
            let depth = reading.tile_size(out);
            if reading.take(out, NESTING) == WHOLE {
                for tile in tiles_of(region, depth) {
                    if reading.value(out) {
                        fill(bits, tile);
                    }
                }
                return;
            }
            // The quadrants the mask covers are bound at the same
            // tile size, one level nearer them; the rest follow as
            // regions, in reading order.
            let mask = reading.take(out, QUADRANTS);
            let children = children_of(region);
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 1 {
                    for tile in tiles_of(child, depth - 1) {
                        if reading.value(out) {
                            fill(bits, tile);
                        }
                    }
                }
            }
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 0 {
                    undescribe(reading, out, bits, child);
                }
            }
        }
        COPY => {
            let dir = reading.take(out, DIRECTION) as usize;
            copy_in(bits, region, DIRECTIONS[dir]);
        }
        MASKED_COPY => {
            let mask = reading.take(out, QUADRANTS);
            let (dx, dy) = DIRECTIONS[reading.take(out, DIRECTION) as usize];
            let children = children_of(region);
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 1 {
                    copy_in(bits, child, (2 * dx, 2 * dy));
                }
            }
            for (bit, &child) in children.iter().enumerate() {
                if mask >> bit & 1 == 0 {
                    undescribe(reading, out, bits, child);
                }
            }
        }
        _ => {
            for child in children_of(region) {
                undescribe(reading, out, bits, child);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples;

    /// A checkerboard of squares `side` cells across.
    fn checkerboard(side: usize) -> BitMatrix {
        let mut bits = BitMatrix::new();
        for y in 0..256 {
            for x in 0..256 {
                if (x / side + y / side) % 2 == 0 {
                    bits.set(x as u8, y as u8);
                }
            }
        }
        bits
    }

    fn cases() -> Vec<BitMatrix> {
        let mut cases = vec![BitMatrix::new()];
        for shape in samples::SHAPES {
            cases.extend(shape.tested());
        }
        cases.extend(samples::grown(0, 1.0, 0.0, 1));
        for side in [1usize, 2, 3, 8, 16] {
            cases.push(checkerboard(side));
        }
        let mut one = BitMatrix::new();
        one.set(128, 128);
        cases.push(one);
        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        cases.push(full);
        cases
    }

    /// The encoding comes back the bitmap that went in. Nothing else
    /// about it matters if this is ever false.
    #[test]
    fn the_encoding_comes_back_the_bitmap_that_went_in() {
        let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
        let (mut out, mut back) = (Encoded::default(), BitMatrix::new());
        for choosing in Choosing::ALL {
            for (case, bits) in cases().iter().enumerate() {
                pyramid.clear();
                pyramid.rebuild(bits);
                encode(&pyramid, bits, choosing, &mut work, &mut out);
                decode(&out, &mut back);
                for y in 0..=u8::MAX {
                    for x in 0..=u8::MAX {
                        assert_eq!(
                            bits.get(x, y),
                            back.get(x, y),
                            "{}, case {case}, differs at ({x}, {y})",
                            choosing.name()
                        );
                    }
                }
            }
        }
    }

    /// A workspace holds the last bitmap's tile sizes where this
    /// one's pruning stopped, so an encode must never read those. The
    /// way to catch it is to encode a plain bitmap after a detailed
    /// one in the same workspace.
    #[test]
    fn a_reused_workspace_does_not_leak_the_last_bitmap() {
        let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
        let (mut out, mut back) = (Encoded::default(), BitMatrix::new());

        pyramid.clear();
        pyramid.rebuild(&checkerboard(1));
        encode(&pyramid, &checkerboard(1), Choosing::LargestHomogeneousTile, &mut work, &mut out);

        let empty = BitMatrix::new();
        pyramid.clear();
        pyramid.rebuild(&empty);
        encode(&pyramid, &empty, Choosing::LargestHomogeneousTile, &mut work, &mut out);
        assert_eq!(out.bits(), UNIT as usize);

        decode(&out, &mut back);
        assert_eq!(back.count_set(), 0);
    }

    /// Every region of a bitmap that holds one thing is homogeneous,
    /// so the whole of it is one code, one tile size, one whole and
    /// one bit.
    #[test]
    fn a_bitmap_of_one_value_is_one_binding() {
        let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
        let mut out = Encoded::default();
        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        for bits in [BitMatrix::new(), full] {
            pyramid.clear();
            pyramid.rebuild(&bits);
            encode(&pyramid, &bits, Choosing::LargestHomogeneousTile, &mut work, &mut out);
            assert_eq!(out.bits(), UNIT as usize);
            assert_eq!(out.counts.whole_bindings, 1);
        }
    }
}
