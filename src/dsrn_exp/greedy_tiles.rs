//! The greedy complex tiler: two passes, greedy tiling then complex
//! tiling, with no bitstream in sight until `tile_or_subdivide.rs`
//! walks whatever tree the second pass produces.
//!
//! **Pass 1, the greedy tiler** (`greedy_tiler`): one rule, asked of
//! every tile size from 256 down to 1, coarsest first, skipping
//! anything a bigger tile already claimed -- homogeneous? bind it.
//! Else copyable (a same-size neighbour, or, one level up, a same-size
//! neighbour of the tile's own parent)? copy it. Else leave it for its
//! four quarters, one size finer, to each try for themselves. No
//! comparison ever happens between sizes; a tile that qualifies is
//! taken immediately. Cells are always homogeneous on their own, so the
//! pass always finishes and always covers the whole bitmap by the time
//! it reaches 1x1. Unchanged in every respect but its own output shape,
//! which is now [`PlacedTiles`] -- one bit-plane (was anything placed
//! here) and one 4-bit-plane (what it says), per level, rather than a
//! flat `Vec` of records -- the same "one element per tile, every
//! level" shape `crate::pyramid::Pyramid` itself is built from,
//! generalized to whatever a plane needs to hold.
//!
//! **Pass 2, the complex tiler** (`complex_tiler`): groups pass 1's own
//! placed tiles into complex tiles -- one aligned area, one chosen
//! resolution, a shared flat list of one-value-bit-each resolution-tile
//! values for whatever fits it, and, for whatever does not, the exact
//! same grammar the top-level tree already has (bind, copy, or further
//! subdivide, at whatever size the underlying content actually is,
//! cascading down as far as it needs to, no floor but the complex
//! tile's own resolution). A `Copied` tile is no longer disqualifying:
//! it is content like any other, masked at its own natural size instead
//! of being routed around. All 1x1 tiles are excluded from the complex
//! tiler unconditionally -- they are always the residual/trailing raw
//! pass's own, exactly matching the top-level tree's own 2x2 floor,
//! where nothing finer ever gets an explicit tree representation
//! either.
//!
//! Never cuts a tile pass 1 placed: every placed tile already lines up
//! with the same power-of-two grid a complex tile candidate does, so it
//! is always either wholly inside a candidate area or wholly outside
//! it. Never nests a complex tile inside another's own body: an area a
//! complex tile masks is read back by the plain tree grammar, which has
//! no way to say "complex" at all -- enforced by construction, a
//! [`Node::Complex`] is only ever built in [`Context::Open`].
//!
//! What a complex tile is worth, and how that is decided, is
//! [`complex_tiler`]'s own doc comment.

use crate::dsrn::region::{same_cells, Region, DIRECTIONS};
use crate::pyramid::{same_tiles, tile_of_bitmap, tile_side, tiles_across, tiles_in_level, Pyramid, CELL_LEVEL};
use crate::Bitmap;

/// What a placed tile says: its own value, or a same-size area to copy,
/// in [`DIRECTIONS`] order -- either a near neighbour of the tile
/// itself, or, one level up, a neighbour of the tile's parent, at the
/// child position the tile itself occupies within its own parent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Says {
    Bound(bool),
    Copied { far: bool, direction: usize },
}

/// How many bits one [`Says`] takes in [`PlacedTiles`]' own 4-bit plane:
/// 1 bit for bind-or-copy, then either 1 value bit (bind, the other 2
/// unused) or 1 far/near bit plus 2 direction bits (copy, using all 3)
/// -- the same widths `tile_or_subdivide.rs` already spends on a leaf's
/// own code and payload, so 4 bits covers both cases exactly.
const SAYS_WIDTH: usize = 4;
const SAYS_PER_WORD: usize = u64::BITS as usize / SAYS_WIDTH;
const PLACED_PER_WORD: usize = u64::BITS as usize;

fn encode_says(says: Says) -> u64 {
    match says {
        Says::Bound(value) => (value as u64) << 1,
        Says::Copied { far, direction } => 0b1 | (far as u64) << 1 | (direction as u64) << 2,
    }
}

fn decode_says(bits: u64) -> Says {
    if bits & 1 == 0 {
        Says::Bound(bits & 0b10 != 0)
    } else {
        Says::Copied { far: bits & 0b10 != 0, direction: ((bits >> 2) & 0b11) as usize }
    }
}

/// What [`greedy_tiler`] placed, one bit-plane (was anything placed
/// here at all) and one [`SAYS_WIDTH`]-bit-plane (what it says) per
/// level -- the pyramid shape generalized to two parallel planes
/// instead of homogeneity's three. One word-packed `Vec<u64>` a level,
/// not one flat array across every level with a boundary table like
/// `Pyramid` itself: this is filled sparsely, tile by tile, as
/// `greedy_tiler` places them, not by scanning every cell, so it does
/// not need that construction-speed optimization.
pub struct PlacedTiles {
    placed: Vec<Vec<u64>>,
    says: Vec<Vec<u64>>,
}

impl PlacedTiles {
    fn new() -> Self {
        let words = |per_word: usize| {
            (0..=CELL_LEVEL).map(move |level| vec![0u64; tiles_in_level(level).div_ceil(per_word)]).collect()
        };
        Self { placed: words(PLACED_PER_WORD), says: words(SAYS_PER_WORD) }
    }

    pub fn is_placed(&self, region: Region) -> bool {
        let idx = region.y * tiles_across(region.level) + region.x;
        (self.placed[region.level][idx / PLACED_PER_WORD] >> (idx % PLACED_PER_WORD)) & 1 != 0
    }

    pub fn get(&self, region: Region) -> Option<Says> {
        if !self.is_placed(region) {
            return None;
        }
        let idx = region.y * tiles_across(region.level) + region.x;
        let shift = (idx % SAYS_PER_WORD) * SAYS_WIDTH;
        let bits = (self.says[region.level][idx / SAYS_PER_WORD] >> shift) & ((1 << SAYS_WIDTH) - 1);
        Some(decode_says(bits))
    }

    fn set(&mut self, region: Region, says: Says) {
        let idx = region.y * tiles_across(region.level) + region.x;
        self.placed[region.level][idx / PLACED_PER_WORD] |= 1 << (idx % PLACED_PER_WORD);
        let shift = (idx % SAYS_PER_WORD) * SAYS_WIDTH;
        self.says[region.level][idx / SAYS_PER_WORD] |= encode_says(says) << shift;
    }

    /// Every placed tile, in reading order within each level, coarsest
    /// level first -- what counting or grouping them needs, without
    /// exposing the planes themselves.
    pub fn iter(&self) -> impl Iterator<Item = (Region, Says)> + '_ {
        (0..=CELL_LEVEL).flat_map(move |level| {
            let across = tiles_across(level);
            (0..across).flat_map(move |y| {
                (0..across).filter_map(move |x| {
                    let region = Region { level, x, y };
                    self.get(region).map(|says| (region, says))
                })
            })
        })
    }
}

/// Runs the greedy tiler over one bitmap, biggest tiles first.
pub fn greedy_tiler(pyramid: &Pyramid, bitmap: &Bitmap) -> PlacedTiles {
    let mut claimed = Bitmap::new();
    let mut placed = PlacedTiles::new();
    let far_copyable = FarCopyable::build(bitmap);

    for level in 0..=CELL_LEVEL {
        let across = tiles_across(level);
        for y in 0..across {
            for x in 0..across {
                let tile = Region { level, x, y };
                let (cx, cy) = tile.top_left_cell();

                // Tiles are placed biggest first and never overlap,
                // so if this tile's corner is already claimed, a
                // coarser tile placed earlier covers the whole of it
                // -- same-size tiles partition the plane, and nothing
                // smaller has run yet.
                if claimed.get(cx as u8, cy as u8) {
                    continue;
                }

                let says = if let Some(value) = tile_of_bitmap(pyramid, bitmap, level, x, y) {
                    Says::Bound(value)
                } else if let Some((far, direction)) = copy_choice(pyramid, &far_copyable, bitmap, tile) {
                    Says::Copied { far, direction }
                } else {
                    // Neither one thing nor a match for any same-size
                    // neighbour: left for this tile's four quarters,
                    // one level finer, to each try for themselves.
                    continue;
                };
                claim(&mut claimed, tile);
                placed.set(tile, says);
            }
        }
    }
    placed
}

/// Which direction a tile copies from, if any, and whether that is a
/// near copy (a same-size neighbour of the tile itself) or a far copy
/// (a same-size neighbour of the tile's parent, at the tile's own
/// child position within it).
fn copy_choice(
    pyramid: &Pyramid,
    far_copyable: &FarCopyable,
    bitmap: &Bitmap,
    tile: Region,
) -> Option<(bool, usize)> {
    if pyramid.copyable(tile.level, tile.x, tile.y) {
        let near = (0..DIRECTIONS.len()).find(|&direction| {
            tile.neighbour(direction).is_some_and(|beside| same_cells(bitmap, tile, beside))
        });
        if let Some(direction) = near {
            return Some((false, direction));
        }
    }
    if !far_copyable.get(tile) {
        return None;
    }
    let parent = Region { level: tile.level - 1, x: tile.x / 2, y: tile.y / 2 };
    let (child_dx, child_dy) = (tile.x % 2, tile.y % 2);
    (0..DIRECTIONS.len()).find_map(|direction| {
        let beside_parent = parent.neighbour(direction)?;
        let far = Region {
            level: tile.level,
            x: beside_parent.x * 2 + child_dx,
            y: beside_parent.y * 2 + child_dy,
        };
        same_cells(bitmap, tile, far).then_some((true, direction))
    })
}

/// One node of the tree pass 2 produces -- the same recursive shape
/// both the top level and a complex tile's own body use, distinguished
/// only by [`Context`], never by a separate type. There is no `Masked`
/// variant: masked content is just whatever this same tree already
/// says about that area (a `Bound`, a `Copied`, or a further `Split`),
/// built once, top-down, by [`complex_tiler`]'s own [`build_node`] --
/// never re-derived at encode time.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Node {
    /// One placed tile's own value.
    Bound(bool),
    /// One placed tile, copying a same-size neighbour.
    Copied { far: bool, direction: usize },
    /// A complex tile: `depth` levels below this region is its own
    /// chosen resolution; `body` is the same question asked of this
    /// whole region in [`Context::Body`]. Only ever built in
    /// [`Context::Open`] -- nesting one complex tile inside another's
    /// body is forbidden, and `body`'s own recursion has no branch that
    /// could produce this variant.
    Complex { depth: usize, body: Box<Node> },
    /// Every resolution-tile under this region, in
    /// [`Region::tiles_at_depth`]'s own reading order for it -- only
    /// ever built in [`Context::Body`], where every one of them is
    /// confirmed to be its own individual `Bound` tile at exactly the
    /// resolution, nothing coarser repeating, nothing finer or copied
    /// needing to be masked out.
    Flat(Vec<bool>),
    /// The same question asked again of this region's own four
    /// children.
    Split(Box<[Node; 4]>),
    /// One level above cells, in place of any of the above: this 2x2
    /// is neither one placed tile nor flat -- its four cells are left
    /// to the trailing raw pass entirely, the same floor the top-level
    /// tree has always had.
    Hole,
}

/// Which grammar a [`Node`] is being read in. The top level and a
/// complex tile's own body read the *same* recursive shape -- leaf, or
/// subdivide into four children of the same context -- but not quite
/// the same leaves: [`Context::Open`] alone may produce `Complex`, and
/// only [`Context::Body`]'s own resolution floor may produce `Flat`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Context {
    /// The top level, or a `Split`'s child of one -- `Bound`, `Copied`,
    /// `Complex`, or `Split`, never `Flat`.
    Open,
    /// Inside a complex tile's body, above its own resolution
    /// (`limit_level`) -- `Bound`, `Copied`, `Flat`, or `Split` of more
    /// `Body`, never `Complex`.
    Body { limit_level: usize },
    /// Inside a complex tile's body, at or below its own resolution --
    /// finer content the resolution does not reach, read exactly like
    /// `Open` except that `Complex` is still forbidden (nesting stays
    /// forbidden all the way down, not just at the resolution itself).
    Plain,
}

/// Groups [`greedy_tiler`]'s own placed tiles into complex tiles, and
/// returns the whole bitmap as one [`Node`] tree -- built once, all the
/// way down, before anything is ever written to a bitstream ("build
/// trees first, then traverse the tree to encode").
///
/// # What a complex tile is worth
///
/// A candidate is a region nothing is placed at directly, tried at
/// every resolution (depth) that could gain anything: from `1` (its own
/// four children) up to the deepest level any `Bound` tile placed under
/// it actually reaches -- coarser loses real content that only exists
/// at the finer level; finer only forces content already fine enough to
/// repeat itself for nothing; both are pure loss, so [`precompute_census`]
/// (a bottom-up count of `Bound` tiles at each level under each region,
/// computed once for the whole bitmap, never per candidate or per
/// round, since it depends only on what `greedy_tiler` already placed)
/// is enough to find every depth actually worth trying without
/// searching a range blind. `1x1` is never a depth worth trying at all
/// -- every 1x1 tile is excluded from the complex tiler unconditionally,
/// always the trailing raw pass's own, which caps a resolution at 2x2
/// and a candidate region at level `CELL_LEVEL - 2`.
///
/// At a given depth, `unmasked` is how many resolution-tiles are
/// genuinely one individual `Bound` tile at exactly that level (nothing
/// coarser repeating into more than one of them, nothing finer or
/// copied needing to be read back separately), and `total` is `4^depth`,
/// the resolution's own tile count over the whole candidate -- masked
/// or not, so a resolution is never rewarded for excluding its way to a
/// good-looking fraction over whatever little is left (excluding
/// everything down to one lonely genuine tile would make a perfect
/// ratio and say almost nothing). Depth `1` never masks (every child
/// already sits at the resolution, so excluding one only adds its own
/// tag on top of what it would have cost standalone, a guaranteed
/// loss) -- it must find `unmasked == total` or it does not qualify at
/// all. Past depth `1`, a floor applies: `4 * unmasked >= 3 * total`,
/// exact integer arithmetic for the same reasoning a floating-point
/// ratio used before -- below it, too little of the resolution is
/// genuinely gathered for the shared header to be worth it over reaching
/// everything by plain subdivision instead. Among the depths that pass,
/// the best is whichever has the highest `unmasked / total`, ties toward
/// the coarser (cheaper in absolute bits for the same fraction) --
/// [`best_resolution`].
///
/// # Choosing between candidates: unmasked area, not ratio, first
///
/// Ranking candidates by ratio first lets a small, ratio-perfect region
/// always pre-empt a bigger region that would need a little masking,
/// even when the bigger one would have absorbed far more in total --
/// measured and confirmed on this exact codebase: a prior version of
/// this pass ranked by ratio and a reclaim capability it had never once
/// won a round on the sample corpus, because whatever needed reclaiming
/// always had some smaller, cleaner sibling region ranked above it.
/// Candidates here are ranked by their own genuinely-captured *cell*
/// area first instead (`unmasked` resolution-tiles at that depth,
/// scaled by that depth's own tile size in cells -- comparable across
/// candidates that end up choosing different resolutions, which a raw
/// resolution-tile count is not), ties toward the better ratio, then
/// the bigger region, then reading order.
///
/// # Committing: one sort, not a round for every commit
///
/// A region's own census, and so its own best depth and score, never
/// changes as other candidates commit -- only whether it is *still
/// available* (its own footprint not already spoken for by an earlier,
/// better-ranked commitment) does. So every candidate is scored once,
/// sorted once by the rule above, and then committed in that order,
/// skipping whatever a `Bitmap` of already-claimed cells says already
/// overlaps -- equivalent to rescanning from scratch every round the way
/// an earlier version of this pass did (each commitment can only ever
/// remove candidates, never add one), but without the repeated work.
pub fn complex_tiler(placed: &PlacedTiles) -> Node {
    let census = precompute_census(placed);

    // Collected biggest tile size first (level 0, the whole bitmap) down
    // to the smallest a candidate can be -- not that the order matters
    // for correctness, since every candidate is scored and sorted before
    // any of them commit, but a bigger area is where the real savings
    // are, so this is the order in which they would be found by hand.
    let mut candidates = Vec::new();
    for level in 0..=(CELL_LEVEL - 2) {
        let across = tiles_across(level);
        for y in 0..across {
            for x in 0..across {
                let region = Region { level, x, y };
                if placed.is_placed(region) {
                    continue; // already one whole tile -- nothing to compose here
                }
                if let Some(candidate) = best_resolution(&census, region) {
                    candidates.push(candidate);
                }
            }
        }
    }
    candidates.sort_by(Candidate::cmp_best_first);

    let mut claimed = Bitmap::new();
    let mut chosen: Vec<Vec<Option<usize>>> =
        (0..=CELL_LEVEL).map(|level| vec![None; tiles_in_level(level)]).collect();
    for candidate in candidates {
        let region = candidate.region;
        let (x, y) = region.top_left_cell();
        let side = region.side_in_cells();
        let (x0, y0, x1, y1) = (x as u8, y as u8, (x + side - 1) as u8, (y + side - 1) as u8);
        // The corner alone would catch a region already wholly inside
        // an earlier, coarser-or-equal commitment (the same shortcut
        // `greedy_tiler`'s own `claimed` check relies on); the full
        // rect is still needed the other way round, when this
        // candidate would itself contain something smaller already
        // committed, which need not touch its corner at all.
        if claimed.get(x0, y0) || claimed.any_set_in_rect(x0, y0, x1, y1) {
            continue;
        }
        claimed.set_rect(x0 as i64, y0 as i64, x1 as i64, y1 as i64);
        let across = tiles_across(region.level);
        chosen[region.level][region.y * across + region.x] = Some(candidate.depth);
    }

    build_node(placed, &chosen, &census, Region::whole_bitmap(), Context::Open)
}

/// One region's own best-scoring resolution, and how it compares
/// against every other candidate -- see [`complex_tiler`]'s own doc
/// comment for what each field means and why.
struct Candidate {
    region: Region,
    depth: usize,
    unmasked: u32,
    total: u32,
}

impl Candidate {
    /// How many actual cells this candidate's own unmasked area covers
    /// -- the fair basis for ranking candidates that end up choosing
    /// different resolutions, since a "resolution tile" is a different
    /// number of cells depending on how deep the resolution goes.
    fn unmasked_cells(&self) -> u64 {
        let side = tile_side(self.region.level + self.depth) as u64;
        self.unmasked as u64 * side * side
    }

    /// Best-first ordering for [`complex_tiler`]'s own commit sweep:
    /// bigger genuinely-captured area first, then better ratio, then
    /// bigger region, then reading order, so the sort is fully
    /// deterministic.
    fn cmp_best_first(a: &Candidate, b: &Candidate) -> std::cmp::Ordering {
        b.unmasked_cells()
            .cmp(&a.unmasked_cells())
            .then_with(|| (b.unmasked as u64 * a.total as u64).cmp(&(a.unmasked as u64 * b.total as u64)))
            .then_with(|| {
                let (a_side, b_side) = (a.region.side_in_cells(), b.region.side_in_cells());
                (b_side * b_side).cmp(&(a_side * a_side))
            })
            .then_with(|| (a.region.level, a.region.y, a.region.x).cmp(&(b.region.level, b.region.y, b.region.x)))
    }
}

/// `region`'s own best depth to try composing at, if any -- see
/// [`complex_tiler`]'s own doc comment for the exact rule.
fn best_resolution(census: &[Vec<[u32; CELL_LEVEL]>], region: Region) -> Option<Candidate> {
    let counts = &census[region.level][region.y * tiles_across(region.level) + region.x];
    let max_depth = CELL_LEVEL - 1 - region.level; // 1x1 (level CELL_LEVEL) is never a valid resolution
    let mut best: Option<Candidate> = None;
    for depth in 1..=max_depth {
        let unmasked = counts[region.level + depth];
        if unmasked == 0 {
            continue;
        }
        let total = 1u32 << (2 * depth);
        if depth == 1 && unmasked != total {
            continue; // masking never pays for itself at depth 1
        }
        if 4 * unmasked < 3 * total {
            continue; // below the floor -- see complex_tiler's own doc comment
        }
        let better = match &best {
            None => true,
            Some(current) => {
                let (lhs, rhs) = (unmasked as u64 * current.total as u64, current.unmasked as u64 * total as u64);
                lhs > rhs || (lhs == rhs && depth < current.depth)
            }
        };
        if better {
            best = Some(Candidate { region, depth, unmasked, total });
        }
    }
    best
}

/// How many `Bound` tiles [`PlacedTiles`] has at exactly level `t`,
/// under every region, for every `t` from `region`'s own level up to
/// `CELL_LEVEL - 1` -- computed bottom-up, once for the whole bitmap.
/// Never recomputed per candidate or per round: a region's own census
/// depends only on what `greedy_tiler` already placed under it, which
/// never changes as `complex_tiler` commits candidates elsewhere.
///
/// A `Bound` tile contributes 1 to its own level and nothing else; a
/// `Copied` tile contributes nothing at any level -- it can never join
/// a complex tile's flat list, only ever be masked at whatever level it
/// already is; nothing placed at this exact region recurses into its
/// own four children and sums what they found. A region's own children
/// at `CELL_LEVEL` (1x1) are never visited -- their row is left at its
/// initial zero, which already contributes nothing when summed, since
/// 1x1 tiles are excluded from the complex tiler unconditionally.
fn precompute_census(placed: &PlacedTiles) -> Vec<Vec<[u32; CELL_LEVEL]>> {
    let mut census: Vec<Vec<[u32; CELL_LEVEL]>> =
        (0..=CELL_LEVEL).map(|level| vec![[0u32; CELL_LEVEL]; tiles_in_level(level)]).collect();
    for level in (0..CELL_LEVEL).rev() {
        let across = tiles_across(level);
        for y in 0..across {
            for x in 0..across {
                let region = Region { level, x, y };
                let idx = y * across + x;
                match placed.get(region) {
                    Some(Says::Bound(_)) => census[level][idx][level] = 1,
                    Some(Says::Copied { .. }) => {}
                    None => {
                        let mut counts = [0u32; CELL_LEVEL];
                        for child in region.children() {
                            if child.level == CELL_LEVEL {
                                continue; // 1x1 -- never counted, see this fn's own doc comment
                            }
                            let child_across = tiles_across(child.level);
                            let child_counts = &census[child.level][child.y * child_across + child.x];
                            for (total, added) in counts.iter_mut().zip(child_counts) {
                                *total += added;
                            }
                        }
                        census[level][idx] = counts;
                    }
                }
            }
        }
    }
    census
}

/// Builds one [`Node`], top-down, for `region` in `context` -- called
/// once, after every candidate [`complex_tiler`] is ever going to
/// commit already has (`chosen`), never interleaved with deciding them.
fn build_node(
    placed: &PlacedTiles,
    chosen: &[Vec<Option<usize>>],
    census: &[Vec<[u32; CELL_LEVEL]>],
    region: Region,
    context: Context,
) -> Node {
    match context {
        Context::Body { limit_level } => build_body_node(placed, chosen, census, region, limit_level),
        Context::Open | Context::Plain => {
            if region.level == CELL_LEVEL - 1 {
                // One level above cells: a homogeneous 2x2 is a `Bound`
                // leaf; anything else is a `Hole`, its four cells left
                // entirely to the trailing raw pass -- the same floor
                // as everywhere else, and, since `Copied` is never
                // this cheap for something this small, `greedy_tiler`
                // never places one here either.
                return match placed.get(region) {
                    Some(Says::Bound(value)) => Node::Bound(value),
                    _ => Node::Hole,
                };
            }
            match placed.get(region) {
                Some(Says::Bound(value)) => Node::Bound(value),
                Some(Says::Copied { far, direction }) => Node::Copied { far, direction },
                None => {
                    let across = tiles_across(region.level);
                    if context == Context::Open {
                        if let Some(depth) = chosen[region.level][region.y * across + region.x] {
                            let limit_level = region.level + depth;
                            let body = build_body_node(placed, chosen, census, region, limit_level);
                            return Node::Complex { depth, body: Box::new(body) };
                        }
                    }
                    Node::Split(Box::new(region.children().map(|child| build_node(placed, chosen, census, child, context))))
                }
            }
        }
    }
}

/// Builds one [`Node`] of a complex tile's own body, for `region`, down
/// to `limit_level` -- the complex tile's own resolution.
fn build_body_node(
    placed: &PlacedTiles,
    chosen: &[Vec<Option<usize>>],
    census: &[Vec<[u32; CELL_LEVEL]>],
    region: Region,
    limit_level: usize,
) -> Node {
    let depth_here = limit_level - region.level;
    let full = 1u32 << (2 * depth_here);
    let across = tiles_across(region.level);
    let idx = region.y * across + region.x;
    if census[region.level][idx][limit_level] == full {
        // Every resolution-tile under here is confirmed, by its own
        // count, to be an individual `Bound` tile at exactly this
        // level -- nothing coarser to repeat, nothing finer or copied
        // to mask out. Already at the resolution (`depth_here == 0`),
        // that is exactly one `Bound` tile, the same as anywhere else
        // in the tree -- `Flat` is reserved for a genuine run of more
        // than one, which only ever happens above the resolution.
        if depth_here == 0 {
            let Some(Says::Bound(value)) = placed.get(region) else {
                unreachable!("a full census of 1 at depth 0 is region's own single Bound tile")
            };
            return Node::Bound(value);
        }
        return Node::Flat(resolution_values(placed, region, limit_level));
    }
    if region.level == limit_level && limit_level == CELL_LEVEL - 1 {
        // The 2x2 floor: never worth a copy's own header for something
        // this small, exactly the same reasoning the top-level tree's
        // own 2x2 floor already applies to a `Copied` tile it finds
        // there -- treated as a hole regardless of whether greedy_tiler
        // placed a `Copied` tile here or nothing lines up at all. A
        // lone `Bound` tile would already have been caught above.
        return Node::Hole;
    }
    if region.level == limit_level {
        // Not flat, and there is nothing finer left within the
        // resolution's own floor to mask out separately -- a lone
        // `Bound` tile here would already have been caught above, so
        // the only other placed thing this can be is a `Copied` tile;
        // otherwise, finer content still exists below, read via
        // `Plain`.
        return match placed.get(region) {
            Some(Says::Copied { far, direction }) => Node::Copied { far, direction },
            Some(Says::Bound(_)) => unreachable!("a lone Bound tile at the resolution is always caught above"),
            None => {
                Node::Split(Box::new(region.children().map(|child| build_node(placed, chosen, census, child, Context::Plain))))
            }
        };
    }
    // Above the resolution, not flat: an existing placed tile here,
    // whatever it is, is masked whole, right here, in one leaf -- the
    // fewest masked areas reclaiming it could ever cost, and, being one
    // placed tile already, no reason to represent it in more than one
    // piece. Otherwise, the same question is asked again of this
    // region's own four children, which is how something reclaimable
    // buried a few levels down still gets isolated without dragging
    // whatever is genuinely flat around it down too.
    match placed.get(region) {
        Some(Says::Bound(value)) => Node::Bound(value),
        Some(Says::Copied { far, direction }) => Node::Copied { far, direction },
        None => Node::Split(Box::new(
            region.children().map(|child| build_body_node(placed, chosen, census, child, limit_level)),
        )),
    }
}

/// Reads every resolution-tile under `region`, at `limit_level`,
/// straight off [`PlacedTiles`] -- only ever called once
/// [`build_body_node`] has already confirmed every one of them is a
/// `Bound` tile at exactly that level, so this can never see a value
/// that needs to repeat.
fn resolution_values(placed: &PlacedTiles, region: Region, limit_level: usize) -> Vec<bool> {
    region
        .tiles_at_depth(limit_level - region.level)
        .into_iter()
        .map(|tile| match placed.get(tile) {
            Some(Says::Bound(value)) => value,
            _ => unreachable!("build_body_node already confirmed every position here is a Bound tile"),
        })
        .collect()
}

/// Whether a tile could far-copy: whether some same-size neighbour of
/// its own *parent*, at the child position this tile occupies within
/// that parent, holds the same cells. Precomputed once a bitmap, for
/// the same reason [`Pyramid::copyable`] is precomputed for near
/// copies -- the question is asked of nearly every tile and the
/// answer is nearly always no.
///
/// A far copy's source sits exactly one *parent* width away in the
/// same direction a near copy's source sits one *tile* width away
/// (the parent a far copy steps to is twice as wide as the tile
/// itself), so this is the same word-level row comparison
/// [`Pyramid`] already builds for near copies, just taken two tiles
/// at a time instead of one.
pub(crate) struct FarCopyable {
    can: Vec<Vec<bool>>,
}

impl FarCopyable {
    pub(crate) fn build(bitmap: &Bitmap) -> Self {
        let can = (0..=CELL_LEVEL)
            .map(|level| {
                let across = tiles_across(level);
                let side = tile_side(level);
                let mut level_bits = vec![false; across * across];
                for y in 0..across {
                    for x in 0..across {
                        level_bits[y * across + x] = DIRECTIONS.iter().any(|&(dx, dy)| {
                            let (ax, ay) = (x as isize + 2 * dx, y as isize + 2 * dy);
                            ax >= 0
                                && ay >= 0
                                && ax < across as isize
                                && ay < across as isize
                                && same_tiles(bitmap, side, (x, y), (ax as usize, ay as usize))
                        });
                    }
                }
                level_bits
            })
            .collect();
        Self { can }
    }

    pub(crate) fn get(&self, tile: Region) -> bool {
        let across = tiles_across(tile.level);
        self.can[tile.level][tile.y * across + tile.x]
    }
}

/// How many tiles the greedy tiler placed, by level, and how many of
/// those were copies rather than a bound value.
///
/// Level 0 is the whole bitmap as one tile; level [`CELL_LEVEL`] is a
/// single cell.
#[derive(Default, Clone, Copy)]
pub struct GreedyTileCounts {
    pub bound_at_level: [usize; CELL_LEVEL + 1],
    pub copied_at_level: [usize; CELL_LEVEL + 1],
}

impl GreedyTileCounts {
    /// Every tile the pass placed, whatever its size or kind.
    pub fn total_tiles(&self) -> usize {
        self.bound_at_level.iter().sum::<usize>() + self.copied_at_level.iter().sum::<usize>()
    }

    /// Every tile that copied rather than carried its own value.
    pub fn total_copies(&self) -> usize {
        self.copied_at_level.iter().sum()
    }
}

/// Runs the greedy tiler and just counts what it placed, by size and
/// kind -- what [`run`] compares against dsrn.
pub fn greedy_tile_pass(pyramid: &Pyramid, bitmap: &Bitmap) -> GreedyTileCounts {
    let mut counts = GreedyTileCounts::default();
    for (region, says) in greedy_tiler(pyramid, bitmap).iter() {
        match says {
            Says::Bound(_) => counts.bound_at_level[region.level] += 1,
            Says::Copied { .. } => counts.copied_at_level[region.level] += 1,
        }
    }
    counts
}

/// Marks every cell of a tile claimed.
fn claim(claimed: &mut Bitmap, tile: Region) {
    let (x, y) = tile.top_left_cell();
    let side = tile.side_in_cells();
    claimed.set_rect(x as i64, y as i64, (x + side - 1) as i64, (y + side - 1) as i64);
}

/// Compares the greedy tiler against the region-based encoder at its
/// best known setting, in the one currency both understand: how many
/// payload-bearing tiles each produces, and at what sizes.
pub fn run() {
    use crate::dsrn::{encode, FourByFour, Knobs, Masking, Workspace};
    use crate::samples;
    use crate::table::Table;

    let knobs = Knobs { masking: Masking::Anywhere, four_by_four: FourByFour::ItsOwnGrammar };
    let (mut pyramid, mut work, mut out) =
        (Pyramid::new(), Workspace::new(), crate::dsrn::Encoded::default());

    for (family, maps) in samples::every_family() {
        let (mut dsrn_bound, mut dsrn_copied) = ([0usize; CELL_LEVEL + 1], [0usize; CELL_LEVEL + 1]);
        let (mut greedy_bound, mut greedy_copied) = ([0usize; CELL_LEVEL + 1], [0usize; CELL_LEVEL + 1]);

        for bitmap in &maps {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            encode(&pyramid, bitmap, knobs, &mut work, &mut out);
            for level in 0..=CELL_LEVEL {
                dsrn_bound[level] += out.counts.bound_tiles_at_level[level];
                dsrn_copied[level] += out.counts.copied_tiles_at_level[level];
            }

            let greedy = greedy_tile_pass(&pyramid, bitmap);
            for level in 0..=CELL_LEVEL {
                greedy_bound[level] += greedy.bound_at_level[level];
                greedy_copied[level] += greedy.copied_at_level[level];
            }
        }

        let n = maps.len();
        println!("\n  {family}, {n} bitmaps. tiles a bitmap, by size.\n");
        let mut t = Table::new(&["tile side\nin cells", "dsrn\nbound", "dsrn\ncopied", "greedy\nbound", "greedy\ncopied"]);
        let (mut dsrn_total, mut greedy_total) = (0usize, 0usize);
        for level in 0..=CELL_LEVEL {
            let side = crate::pyramid::tile_side(level);
            t.row(&[
                format!("{side}x{side}"),
                (dsrn_bound[level] / n).to_string(),
                (dsrn_copied[level] / n).to_string(),
                (greedy_bound[level] / n).to_string(),
                (greedy_copied[level] / n).to_string(),
            ]);
            dsrn_total += dsrn_bound[level] + dsrn_copied[level];
            greedy_total += greedy_bound[level] + greedy_copied[level];
        }
        t.print();
        println!(
            "\n  total tiles a bitmap: dsrn {}, greedy {} ({:+.1}%)",
            dsrn_total / n,
            greedy_total / n,
            100.0 * (greedy_total as f64 - dsrn_total as f64) / dsrn_total as f64
        );
    }
}
