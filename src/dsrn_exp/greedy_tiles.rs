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
//! resolution. Every part of it is either related to it (unmasked: a
//! `Bound` tile at exactly its resolution, its value in the complex
//! tile's payload) or not (masked): related to a complex tile
//! enclosing it instead, a copy, a complex tile nested inside it at
//! another resolution, or further subdivided. The outer complex tiles
//! capture the coarse structure; the areas they mask get nested complex
//! tiles of their own, round after round. Any `Bound` tile left related
//! to nothing is a complex tile whose resolution is its own size -- just
//! a *tile* -- so the whole plane is tiled with complex tiles and there
//! is no separate "simple bind". All 1x1 tiles are excluded from the
//! complex tiler unconditionally -- they are always the residual pass's
//! own, matching the tree's own 2x2 floor.
//!
//! Never looks at the bitmap: every decision is made from the tiles
//! `greedy_tiler` placed. Never cuts a placed tile: every placed tile
//! lines up with the same power-of-two grid a complex tile does, so it
//! is always wholly inside a complex tile or wholly outside it.
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

/// One node of the tree pass 2 produces. The whole plane is tiled with
/// complex tiles: every placed `Bound` tile is either related to a
/// complex tile enclosing it or is a complex tile of its own whose
/// resolution is its own size -- just a *tile* (`Complex` at depth 0).
/// So there is no separate "simple bind": a bind is always a complex
/// tile, and its resolution says which kind.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Node {
    /// Related to (unmasked in) one of the complex tiles enclosing this
    /// region: `nesting` names which, `0` being the outermost. Every
    /// tile of that complex tile's resolution under this region, in
    /// [`Region::tiles_at_depth`] order -- said in that complex tile's
    /// own payload, after its body, not here.
    Related { nesting: usize, values: Vec<bool> },
    /// One placed tile, copying a same-size neighbour.
    Copied { far: bool, direction: usize },
    /// A complex tile: its resolution is `depth` levels below this
    /// region, and `body` says this same region in terms of it -- a
    /// whole-region `Related` to this complex tile itself when nothing
    /// in it is masked (always so at depth 0 and 1), otherwise a `Split`
    /// whose nodes are related to it, related to one enclosing it, or
    /// masked in the ordinary way -- a copy, another complex tile nested
    /// inside it, or further subdivision.
    Complex { depth: usize, body: Box<Node> },
    /// The same question asked again of this region's four children.
    Split(Box<[Node; 4]>),
    /// One level above cells: a 2x2 that is not one placed `Bound`
    /// tile, its four cells left to the residual pass.
    Hole,
}

/// Groups [`greedy_tiler`]'s own placed tiles into complex tiles, nested
/// as deep as they keep paying, and returns the whole bitmap as one
/// [`Node`] tree -- built once, all the way down, before anything is
/// ever written to a bitstream ("build trees first, then traverse the
/// tree to encode"). Never looks at the bitmap: every decision is made
/// from what `greedy_tiler` placed.
///
/// # Rounds
///
/// The first round searches the whole bitmap for complex tiles that
/// capture as much coarse structure as possible. Each later round
/// searches only inside the complex tiles the round before it
/// committed, for complex tiles nested in them -- a different
/// resolution for the areas the enclosing one masks. Rounds stop when
/// one commits nothing. Whatever placed `Bound` tile ends up related to
/// no complex tile becomes a tile of its own when the tree is built.
///
/// # What a candidate is worth
///
/// A candidate is a region nothing is placed at directly, not already
/// entirely related to a complex tile enclosing it, tried at every depth
/// from `1` to the 2x2 floor (a 1x1 resolution is never tried: 1x1
/// tiles are always the residual pass's own). A depth whose resolution
/// is already an enclosing complex tile's is skipped: those tiles are
/// already related to it. At a given depth, `unmasked_cells` is how
/// many cells the `Bound` tiles placed at exactly that resolution cover;
/// `total_cells` is the region's own cells minus whatever is already
/// related to an enclosing complex tile -- what that one says costs the
/// candidate nothing, so it does not count against it either. Depth `1`
/// must have all four children unmasked (masking never pays there);
/// deeper, `4 * unmasked_cells >= 3 * total_cells`. Since `total_cells`
/// is the same at every depth, the best depth is simply the one with the
/// most unmasked cells, ties toward the coarser.
///
/// # Choosing between candidates: unmasked area first, not ratio
///
/// Every round's candidates are sorted once, by unmasked cells first,
/// then ratio, then region size, then reading order, and committed in
/// that order, skipping any overlapping one already committed this
/// round. Ranking by ratio first lets a small, ratio-perfect region
/// always pre-empt a bigger one that needs a little masking -- measured
/// on this codebase: a reclaim capability ranked that way never once
/// won on the sample corpus.
pub fn complex_tiler(placed: &PlacedTiles) -> Node {
    let census = precompute_census(placed);
    let mut chosen: Vec<Vec<Option<usize>>> =
        (0..=CELL_LEVEL).map(|level| vec![None; tiles_in_level(level)]).collect();

    // Where the next round searches: an area, the finest level it
    // starts from, and the resolutions of the complex tiles enclosing
    // it, outermost first.
    let mut searched_next = vec![SearchArea { area: Region::whole_bitmap(), first_level: 0, enclosing: Vec::new() }];
    while !searched_next.is_empty() {
        let mut candidates = Vec::new();
        for search in &searched_next {
            // Biggest tile size first, down to the smallest a candidate
            // can be (4x4, so its resolution is at least 2x2).
            for level in search.first_level..=(CELL_LEVEL - 2) {
                for region in search.area.tiles_at_depth(level - search.area.level) {
                    if placed.is_placed(region) || related_to_enclosing(&census, region, &search.enclosing).is_some() {
                        continue;
                    }
                    if let Some(candidate) = best_resolution(&census, region, &search.enclosing) {
                        candidates.push(candidate);
                    }
                }
            }
        }
        candidates.sort_by(Candidate::cmp_best_first);

        let mut claimed = Bitmap::new();
        searched_next = Vec::new();
        for candidate in candidates {
            let region = candidate.region;
            let (x, y) = region.top_left_cell();
            let side = region.side_in_cells();
            let (x0, y0, x1, y1) = (x as u8, y as u8, (x + side - 1) as u8, (y + side - 1) as u8);
            // The corner alone catches a region already wholly inside
            // an earlier, coarser-or-equal commitment; the full rect is
            // still needed the other way round, when this candidate
            // would contain something smaller already committed.
            if claimed.get(x0, y0) || claimed.any_set_in_rect(x0, y0, x1, y1) {
                continue;
            }
            claimed.set_rect(x0 as i64, y0 as i64, x1 as i64, y1 as i64);
            chosen[region.level][region.y * tiles_across(region.level) + region.x] = Some(candidate.depth);
            let mut enclosing = candidate.enclosing;
            enclosing.push(region.level + candidate.depth);
            searched_next.push(SearchArea { area: region, first_level: region.level + 1, enclosing });
        }
    }

    build_node(placed, &chosen, &census, Region::whole_bitmap(), &mut Vec::new())
}

/// One round's search area -- see [`complex_tiler`].
struct SearchArea {
    area: Region,
    first_level: usize,
    enclosing: Vec<usize>,
}

/// One region's own best-scoring resolution, and how it compares
/// against every other candidate -- see [`complex_tiler`]'s own doc
/// comment for what each field means and why.
struct Candidate {
    region: Region,
    depth: usize,
    unmasked_cells: u64,
    total_cells: u64,
    /// The resolutions of the complex tiles enclosing `region`,
    /// outermost first.
    enclosing: Vec<usize>,
}

impl Candidate {
    /// Best-first ordering for [`complex_tiler`]'s own commit sweep:
    /// more unmasked cells first, then better ratio, then bigger
    /// region, then reading order, so the sort is fully deterministic.
    fn cmp_best_first(a: &Candidate, b: &Candidate) -> std::cmp::Ordering {
        b.unmasked_cells
            .cmp(&a.unmasked_cells)
            .then_with(|| (b.unmasked_cells * a.total_cells).cmp(&(a.unmasked_cells * b.total_cells)))
            .then_with(|| a.region.level.cmp(&b.region.level))
            .then_with(|| (a.region.y, a.region.x).cmp(&(b.region.y, b.region.x)))
    }
}

/// How many cells one tile at `level` covers.
fn cells_in_tile(level: usize) -> u64 {
    let side = tile_side(level) as u64;
    side * side
}

/// Whether every tile of `resolution` under `region` is a `Bound` tile
/// placed at exactly that level -- what being related to a complex tile
/// of that resolution needs.
fn entirely_bound_at(census: &[Vec<[u32; CELL_LEVEL]>], region: Region, resolution: usize) -> bool {
    let counts = &census[region.level][region.y * tiles_across(region.level) + region.x];
    counts[resolution] == 1 << (2 * (resolution - region.level))
}

/// Which enclosing complex tile, if any, `region` is entirely related
/// to -- the nearest one that can be, as the relation bits ask them.
/// `enclosing` holds their resolutions, outermost first; one coarser
/// than `region` itself cannot relate it.
fn related_to_enclosing(census: &[Vec<[u32; CELL_LEVEL]>], region: Region, enclosing: &[usize]) -> Option<usize> {
    (0..enclosing.len())
        .rev()
        .find(|&nesting| region.level <= enclosing[nesting] && entirely_bound_at(census, region, enclosing[nesting]))
}

/// `region`'s own best depth to try composing at, if any -- see
/// [`complex_tiler`]'s own doc comment for the exact rule.
fn best_resolution(census: &[Vec<[u32; CELL_LEVEL]>], region: Region, enclosing: &[usize]) -> Option<Candidate> {
    let counts = &census[region.level][region.y * tiles_across(region.level) + region.x];
    let related_above: u64 = enclosing
        .iter()
        .filter(|&&resolution| resolution >= region.level)
        .map(|&resolution| counts[resolution] as u64 * cells_in_tile(resolution))
        .sum();
    let total_cells = cells_in_tile(region.level) - related_above;
    let max_depth = CELL_LEVEL - 1 - region.level; // 1x1 is never a resolution
    let mut best: Option<Candidate> = None;
    for depth in 1..=max_depth {
        let resolution = region.level + depth;
        if enclosing.contains(&resolution) {
            continue; // those tiles are already related to that complex tile
        }
        let unmasked = counts[resolution];
        if unmasked == 0 || (depth == 1 && unmasked != 4) {
            continue; // nothing to gain, or masking at depth 1, which never pays
        }
        let unmasked_cells = unmasked as u64 * cells_in_tile(resolution);
        if 4 * unmasked_cells < 3 * total_cells {
            continue; // below the floor
        }
        if best.as_ref().is_none_or(|current| unmasked_cells > current.unmasked_cells) {
            best = Some(Candidate { region, depth, unmasked_cells, total_cells, enclosing: enclosing.to_vec() });
        }
    }
    best
}

/// How many `Bound` tiles [`PlacedTiles`] has at exactly level `t`,
/// under every region, for every `t` from `region`'s own level up to
/// `CELL_LEVEL - 1` -- computed bottom-up, once for the whole bitmap.
/// Never recomputed per candidate or per round: a region's own census
/// depends only on what `greedy_tiler` already placed under it.
///
/// A `Bound` tile contributes 1 to its own level and nothing else; a
/// `Copied` tile contributes nothing at any level; nothing placed at
/// this exact region sums its four children. 1x1 children are never
/// counted: 1x1 tiles are always the residual pass's own.
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
                                continue;
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

/// Builds one [`Node`], top-down, for `region` -- called once, after
/// every complex tile [`complex_tiler`] is ever going to commit already
/// has (`chosen`), never interleaved with deciding them. `enclosing`
/// holds the resolutions of the complex tiles enclosing `region`,
/// outermost first.
fn build_node(
    placed: &PlacedTiles,
    chosen: &[Vec<Option<usize>>],
    census: &[Vec<[u32; CELL_LEVEL]>],
    region: Region,
    enclosing: &mut Vec<usize>,
) -> Node {
    if let Some(nesting) = related_to_enclosing(census, region, enclosing) {
        return Node::Related { nesting, values: resolution_values(placed, region, enclosing[nesting]) };
    }
    let tile = |value: bool, nesting: usize| Node::Complex {
        depth: 0,
        body: Box::new(Node::Related { nesting, values: vec![value] }),
    };
    if region.level == CELL_LEVEL - 1 {
        // One level above cells: a homogeneous 2x2 is a tile; anything
        // else is a hole, its four cells left to the residual pass.
        return match placed.get(region) {
            Some(Says::Bound(value)) => tile(value, enclosing.len()),
            _ => Node::Hole,
        };
    }
    match placed.get(region) {
        Some(Says::Bound(value)) => tile(value, enclosing.len()),
        Some(Says::Copied { far, direction }) => Node::Copied { far, direction },
        None => match chosen[region.level][region.y * tiles_across(region.level) + region.x] {
            Some(depth) => {
                let resolution = region.level + depth;
                let nesting = enclosing.len();
                let body = if entirely_bound_at(census, region, resolution) {
                    Node::Related { nesting, values: resolution_values(placed, region, resolution) }
                } else {
                    enclosing.push(resolution);
                    let children = region.children().map(|child| build_node(placed, chosen, census, child, enclosing));
                    enclosing.pop();
                    Node::Split(Box::new(children))
                };
                Node::Complex { depth, body: Box::new(body) }
            }
            None => Node::Split(Box::new(region.children().map(|child| build_node(placed, chosen, census, child, enclosing)))),
        },
    }
}

/// Reads every tile of `resolution` under `region` straight off
/// [`PlacedTiles`] -- only ever called once `region` is known to be
/// entirely `Bound` tiles at exactly that level.
fn resolution_values(placed: &PlacedTiles, region: Region, resolution: usize) -> Vec<bool> {
    region
        .tiles_at_depth(resolution - region.level)
        .into_iter()
        .map(|tile| match placed.get(tile) {
            Some(Says::Bound(value)) => value,
            _ => unreachable!("only called on a region entirely Bound at this resolution"),
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
