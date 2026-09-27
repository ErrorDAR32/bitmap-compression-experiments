//! An alternate algorithm to DSRN's own: a flat, size-ordered greedy
//! tile placement, with no region tree at all.
//!
//! DSRN decides a region's whole subtree at once, comparing the exact
//! cost of every way it could go before committing to any of it. This
//! is the opposite kind of pass: there are no regions, no standing, no
//! recursive cost tables. There is a plane and a size, biggest first,
//! and one rule at each size: a tile that is one thing, or that holds
//! the same cells as a same-size neighbour, gets placed and claimed; a
//! tile that is neither is left for the next, finer size to try on its
//! own four quarters. Both checks read the bitmap directly and answer
//! at once -- the bitmap never changes, so there is nothing for either
//! one to wait on, whatever order tiles get visited in. Tiles never
//! overlap, so a size that has already claimed a tile is a size no
//! finer pass ever has to look at again, and by the time the pass
//! reaches 1x1 every remaining cell is, on its own, one thing -- so
//! the pass always finishes and always covers the whole bitmap.
//!
//! This file only decides which tiles that rule would place. It does
//! not write a bitstream: there is nothing here yet that says how a
//! tile's own size, position and kind (bound to a value, or copying a
//! direction) would be spelled out in bits, only how many of each
//! kind there would be. That is the thing worth counting first,
//! against what the region-based encoder actually produces, before
//! anything is spent on a grammar for it.

use crate::dsrn::region::{same_cells, Region, DIRECTIONS};
use crate::pyramid::{same_tiles, tile_of_bitmap, tile_side, tiles_across, tiles_in_level, Pyramid, CELL_LEVEL};
use crate::Bitmap;

/// The least of a masked candidate's own resolution that has to be
/// genuinely gathered, not excluded, before [`compose_complex_tiles`]
/// will consider it at all. See the doc comment where it is used.
const THREE_QUARTERS_GENUINE: f64 = 0.75;

/// How many tiles the greedy pass placed, by level, and how many of
/// those were copies or complex tiles rather than a bound value.
///
/// Level 0 is the whole bitmap as one tile; level [`CELL_LEVEL`] is a
/// single cell. A tile at level `CELL_LEVEL` is never a copy or a
/// complex tile -- there is nothing smaller for its one cell to say
/// but its own bit.
#[derive(Default, Clone, Copy)]
pub struct GreedyTileCounts {
    pub bound_at_level: [usize; CELL_LEVEL + 1],
    pub copied_at_level: [usize; CELL_LEVEL + 1],
    pub complex_at_level: [usize; CELL_LEVEL + 1],
}

impl GreedyTileCounts {
    /// Every tile the pass placed, whatever its size or kind.
    pub fn total_tiles(&self) -> usize {
        self.bound_at_level.iter().sum::<usize>()
            + self.copied_at_level.iter().sum::<usize>()
            + self.complex_at_level.iter().sum::<usize>()
    }

    /// Every tile that copied rather than carried its own value.
    pub fn total_copies(&self) -> usize {
        self.copied_at_level.iter().sum()
    }

    /// Every tile that was a complex tile rather than one value.
    pub fn total_complex(&self) -> usize {
        self.complex_at_level.iter().sum()
    }
}

/// A tile the greedy pass placed: where, and what it says about
/// itself.
#[derive(Clone)]
pub struct PlacedTile {
    pub region: Region,
    pub says: Says,
}

/// What a placed tile says: its own value, a same-size area to copy,
/// in [`crate::dsrn::region::DIRECTIONS`] order -- either a near
/// neighbour of the tile itself, or, one level up, a neighbour of the
/// tile's parent, at the child position the tile itself occupies
/// within its own parent -- or, when it is neither one thing nor a
/// match for a neighbour, a tile-aligned area made of several smaller
/// same-size tiles instead.
#[derive(Clone)]
pub enum Says {
    Bound(bool),
    Copied { far: bool, direction: usize },
    /// `depth` levels below this region is the resolution every
    /// `Unmasked` area of `mask` settles on -- one [`MaskNode`] for
    /// each of this region's own four children, in [`Region::children`]
    /// order, `Unmasked` for all four when there is no masking at all.
    Complex { depth: usize, mask: [MaskNode; 4] },
}

/// One node of a complex tile's mask, over the area it covers.
///
/// A masked area is read as a plain region of its own, right where it
/// sits in the tree -- nesting a complex tile inside it is forbidden,
/// so it can only ever turn out `Bound`, `Copied`, or plain
/// subdivision -- but that does not forbid a masked area's own
/// children from being unmasked in turn: `Subdivided` asks the same
/// question again of an area's own four children, at half its size and
/// a quarter its area, which is what lets a masked area still give
/// part of what is inside it back to the complex tile, however many
/// times that happens on the way down. It never goes lower than the
/// complex tile's own tile size -- there is nothing finer left for the
/// complex tile itself to say about it -- so a node already at that
/// size is always `Unmasked` or `Masked`, never `Subdivided`.
///
/// A `Masked` area can be any size, not only the complex tile's own
/// direct-child size, since the tiles enclosed inside a complex tile
/// come from the same greedy, size-ordered placement as everything
/// else -- there is no reason for what gets excluded to line up with
/// the complex tile's own four quarters any more than what gets
/// absorbed does. The one thing that never changes size is the
/// complex tile's own tile size, `Masked`'s floor: a masked area is
/// always [`decide_tiles`]' own placement, or a whole area of several
/// of its tiles none of which, on their own, reach that resolution --
/// never something [`build_mask_node`] had to guess about, and never
/// an existing complex tile, since nesting one inside another's mask
/// is forbidden and there is nothing finer to isolate it into once its
/// own single, whole grid entry is reached.
#[derive(Clone)]
pub enum MaskNode {
    /// This whole area belongs to the complex tile: its own resolution
    /// tile values, in the reading order [`Region::tiles_at_depth`]
    /// uses for this exact area.
    Unmasked(Vec<bool>),
    /// This whole area is excluded, read as a plain region of its own.
    Masked,
    /// The same question asked again of this area's own four children.
    Subdivided(Box<[MaskNode; 4]>),
}

/// Runs the greedy pass over one bitmap, biggest tiles first.
pub fn decide_tiles(pyramid: &Pyramid, bitmap: &Bitmap) -> Vec<PlacedTile> {
    let mut claimed = Bitmap::new();
    let mut placed = Vec::new();
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
                } else if let Some((far, direction)) =
                    copy_choice(pyramid, &far_copyable, bitmap, tile)
                {
                    // A same-size area holds the same cells, read
                    // straight from the bitmap -- true or false the
                    // moment it is asked, regardless of whether that
                    // area is itself one tile or several. Whether a
                    // stream can actually deliver that is a question
                    // for whoever writes cells out, not for deciding
                    // what the tile space looks like.
                    Says::Copied { far, direction }
                } else {
                    // Neither one thing nor a match for any same-size
                    // neighbour: left for this tile's four quarters,
                    // one level finer, to each try for themselves.
                    continue;
                };
                claim(&mut claimed, tile);
                placed.push(PlacedTile { region: tile, says });
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

/// Groups [`decide_tiles`]'s own placed tiles into complex tiles: an
/// aligned area, 4x4 or coarser, covered by `Bound` tiles none finer
/// than 2x2 -- except wherever some smaller area inside it is masked
/// out instead, which excludes just that area rather than disqualifying
/// the rest -- said once at the coarsest resolution that still covers
/// every one of its unmasked tiles, the smallest of their own sizes. A
/// tile bigger than that resolution decomposes into that many repeats
/// of its own value; nothing is ever cut, since every placed tile
/// already lines up with the same power-of-two grid a candidate area
/// does, so it is always either wholly inside that area or wholly
/// outside it, never straddling the edge. A masked area is left exactly
/// as it stood before this tile composed, and forever excluded from
/// composing into a complex tile of its own -- no nesting, at least for
/// a first version of this -- so it can only ever turn out `Bound`,
/// `Copied`, or plain subdivision, read as a region of its own wherever
/// this tile is written out. Only `Bound` tiles ever compose, so a tile
/// this pass just placed -- `Complex` -- never composes again into a
/// coarser one, masked or not.
///
/// A masked area can be any size, found by [`build_mask_node`] the same
/// way [`decide_tiles`] itself found tiles: try the whole area first,
/// and only if that does not work, ask the same question again of its
/// own four children, at half the size. What forces that recursion is
/// either an existing `Copied` tile or a 1x1 remnant somewhere inside
/// -- routable around by masking, since neither one ever needs the
/// complex-flag bit a masked area's own recursive encoding skips -- or
/// an existing complex tile, which is never routable around: nesting
/// one inside another's mask is forbidden, and an existing complex
/// tile is always one whole grid entry with nothing finer beneath it to
/// isolate the rest of the candidate away from, so hitting one makes
/// the whole candidate, at this depth, impossible rather than merely
/// something to mask. [`gather`] tells these two apart, and
/// [`build_mask_node`] never lets the second one bubble up as a mask.
///
/// The naive greedy search this settled on, rather than biggest area
/// first with no comparison: every valid area, of every size and
/// position, is tried at every resolution it could possibly settle on,
/// coarsest tile size to finest, and the round commits exactly one --
/// the one absorbing the most of `decide_tiles`' own tiles for the
/// fewest tiles the resolution names in total, `constituents / (side *
/// side)`, breaking a tie toward the larger area. `side * side` is the
/// resolution's own tile count over the *whole* candidate, masked or
/// not -- unlike an ordinary complex tile's payload, this never shrinks
/// just because something was excluded, or masking would always look
/// free and nothing would ever stop it from excluding everything down
/// to what technically gathers best, however little that leaves to
/// actually say. Counting the full area instead means masking only ever
/// pays for itself when what it excludes was dragging the ratio down by
/// more than the area it costs to give up -- a small obstruction in an
/// otherwise-uniform area is nearly free to route around; excluding
/// most of the area for the sake of a good ratio over what little is
/// left is not, since the parts given up still count against it.
/// Picking one candidate can only remove others -- an area it just
/// absorbed cannot be gathered into anything else, and neither can
/// whatever it left masked, forever -- never add one, so re-scanning
/// every candidate from scratch each round is wasteful but never wrong,
/// and the round after nothing qualifies is where this stops.
///
/// This never looks at the bitmap, or at cells as such -- only at
/// tiles decide_tiles already placed, whatever their size.
pub fn compose_complex_tiles(tiles: Vec<PlacedTile>) -> Vec<PlacedTile> {
    let mut grid: Vec<Vec<Option<Says>>> =
        (0..=CELL_LEVEL).map(|level| vec![None; tiles_in_level(level)]).collect();
    for PlacedTile { region, says } in tiles {
        let across = tiles_across(region.level);
        grid[region.level][region.y * across + region.x] = Some(says);
    }

    // Everything a masked area ever covers, at every level down to
    // cells -- set the moment `build_mask_node` settles on `Masked` for
    // it, and never cleared, so no later round can compose any of it
    // into a complex tile of its own. No nesting, at least for a first
    // version of this: a masked area only ever gives part of itself
    // back to the one complex tile that masked it in the first place,
    // by unmasking one of its own children in turn -- never by becoming
    // a whole new complex tile of its own.
    let mut excluded: Vec<Vec<bool>> = (0..=CELL_LEVEL).map(|level| vec![false; tiles_in_level(level)]).collect();

    // Every possible complex tile, tried again from scratch each round:
    // committing one can only ever remove candidates (an area it just
    // absorbed can no longer be gathered into anything else), never add
    // one, so re-scanning is wasteful but never wrong.
    loop {
        // `gather`'s own result for a region never depends on the
        // `limit_level` a candidate happens to be trying, only on
        // `grid` and `excluded` -- both fixed for the whole of one
        // round -- so it is computed once a round, bottom-up, rather
        // than fresh for every depth of every candidate region, which
        // is what an unbounded, per-depth recursive `gather` would
        // otherwise cost: the same subtree, walked again from scratch,
        // for every one of up to eight depths, at every position still
        // open to composing.
        let gathered = precompute_gathered(&grid, &excluded);

        // The one resolution actually worth trying for a region: the
        // deepest level any of its own unobstructed `Bound` tiles
        // already reaches, ignoring whatever a `Copied` or `Complex`
        // tile in the way would need (masked out, they need no
        // resolution of their own). Any resolution coarser than this
        // would lose real content; anything finer only forces content
        // that is already fine enough to repeat itself for nothing --
        // both are pure loss, so unlike `gathered`, only ever this one
        // depth a region is worth trying at all, never a range to
        // search.
        let natural_finest = precompute_natural_finest(&grid, &excluded);

        let mut best: Option<(f64, usize, Region, usize, [MaskNode; 4])> = None;
        for level in 0..=(CELL_LEVEL - 2) {
            let across = tiles_across(level);
            for y in 0..across {
                for x in 0..across {
                    let idx = y * across + x;
                    if grid[level][idx].is_some() || excluded[level][idx] {
                        // Already one placed tile, a complex tile from
                        // an earlier round, or forever excluded by an
                        // ancestor complex tile's own mask -- nothing
                        // left here for a coarser one to find.
                        continue;
                    }
                    let Some(limit_level) = natural_finest[level][idx] else { continue };
                    let region = Region { level, x, y };
                    let depth = limit_level - level;
                    let mut nodes: [Option<MaskNode>; 4] = [None, None, None, None];
                    let mut constituents = 0usize;
                    let mut possible = true;
                    for (i, child) in region.children().into_iter().enumerate() {
                        let Some((node, node_constituents)) = build_mask_node(&grid, &gathered, child, limit_level)
                        else {
                            // An existing complex tile sits somewhere
                            // in here with no room left to isolate
                            // around it -- this region is impossible.
                            possible = false;
                            break;
                        };
                        constituents += node_constituents;
                        nodes[i] = Some(node);
                    }
                    let all_unmasked = nodes.iter().all(|node| matches!(node, Some(MaskNode::Unmasked(_))));
                    if !possible || constituents == 0 {
                        continue; // nothing gained at this resolution at all
                    }
                    if depth == 1 && !all_unmasked {
                        // Masking a whole direct child pays for itself
                        // only once the shared header is amortized over
                        // decomposing it into more than one payload
                        // tile -- at depth 1 a child is never
                        // decomposed at all (it already is the
                        // resolution), so excluding one here can only
                        // ever shrink the header's own payoff and add
                        // the excluded child's own tag bit on top, a
                        // net loss every time, whatever it is that
                        // would have been excluded.
                        continue;
                    }
                    let side = 1usize << depth;
                    let payload = side * side;
                    let ratio = constituents as f64 / payload as f64;
                    if ratio < THREE_QUARTERS_GENUINE {
                        // Masking still pays a small, fixed tax on the
                        // header's own amortization for every child it
                        // excludes (see the depth == 1 case above, where
                        // that tax is never worth paying at all): below
                        // this floor, too little of the resolution is
                        // genuinely gathered for the shared header to be
                        // worth it over reaching everything by plain
                        // subdivision instead, whatever the untaxed
                        // ratio alone suggests. Not a tight bound -- an
                        // exact one would mean pricing every candidate
                        // against its own real alternative, the
                        // subtree-by-subtree cost comparison this
                        // module exists specifically to avoid -- just
                        // the cheapest floor that stopped a real,
                        // measured regression on bitmaps with plenty of
                        // small, un-composable content in the way.
                        continue;
                    }
                    let area = region.side_in_cells() * region.side_in_cells();
                    let better = match best {
                        Some((best_ratio, best_area, ..)) => {
                            ratio > best_ratio || (ratio == best_ratio && area > best_area)
                        }
                        None => true,
                    };
                    if better {
                        best = Some((ratio, area, region, depth, nodes.map(Option::unwrap)));
                    }
                }
            }
        }
        let Some((_, _, region, depth, nodes)) = best else { break };
        for (child, node) in region.children().into_iter().zip(&nodes) {
            consume_mask_tree(&mut grid, child, node);
            exclude_mask_tree(&mut excluded, child, node);
        }
        let across = tiles_across(region.level);
        grid[region.level][region.y * across + region.x] = Some(Says::Complex { depth, mask: nodes });
    }

    grid.into_iter()
        .enumerate()
        .flat_map(|(level, row)| {
            let across = tiles_across(level);
            row.into_iter().enumerate().filter_map(move |(i, says)| {
                let region = Region { level, x: i % across, y: i / across };
                says.map(|says| PlacedTile { region, says })
            })
        })
        .collect()
}

/// What [`gather`] found over a region: every tile under it is `Bound`
/// (`Whole`); something under it can never take part in a complex tile
/// at all but is routable around by masking, an existing `Copied` tile
/// or a 1x1 remnant (`Disqualified`); or something under it is an
/// existing complex tile, which is never routable around -- nesting one
/// inside another's mask is forbidden, and it is always one whole grid
/// entry with nothing finer beneath it to isolate the rest of a
/// candidate away from (`Blocked`). The distinction is what keeps
/// [`build_mask_node`] from ever masking over an existing complex tile
/// the way it safely can an existing `Copied` tile: `Blocked` has to
/// win over `Disqualified` whenever both turn up among a region's own
/// children, since a sibling that merely disqualifies must never hide
/// one that outright blocks.
enum Gathered {
    Whole { finest: usize, constituents: Vec<(Region, bool)> },
    Disqualified,
    Blocked,
}

/// [`Gathered`] for every region, bottom-up: whether every placed tile
/// under it is `Bound` and none of them finer than 2x2, and if so, the
/// finest level among them and every one of them, region and value, in
/// no particular order -- otherwise, whether an existing complex tile
/// is anywhere in the way or this is merely ordinary disqualified
/// content. `excluded` disqualifies a region at every level, not only
/// where a mask tree first named it: an area some earlier round already
/// masked must never be gathered into a different complex tile's own
/// payload. Computed once for the whole grid, since the answer for a
/// region depends only on `grid` and `excluded`, both fixed for the
/// round this is called from -- never on the `limit_level` whatever
/// candidate composing them happens to be trying, so recomputing it
/// once for every depth of every still-open position, the way a
/// recursive `gather` starting fresh each time would, would only ever
/// walk the same unchanged subtrees again.
fn precompute_gathered(grid: &[Vec<Option<Says>>], excluded: &[Vec<bool>]) -> Vec<Vec<Gathered>> {
    let mut cache: Vec<Vec<Gathered>> = (0..=CELL_LEVEL).map(|_| Vec::new()).collect();
    for level in (0..=CELL_LEVEL).rev() {
        let across = tiles_across(level);
        let mut row = Vec::with_capacity(across * across);
        for y in 0..across {
            for x in 0..across {
                let region = Region { level, x, y };
                let idx = y * across + x;
                row.push(if level == CELL_LEVEL || excluded[level][idx] {
                    Gathered::Disqualified // a 1x1 tile never takes part in a complex tile
                } else {
                    match &grid[level][idx] {
                        Some(Says::Bound(value)) => {
                            Gathered::Whole { finest: level, constituents: vec![(region, *value)] }
                        }
                        Some(Says::Copied { .. }) => Gathered::Disqualified,
                        Some(Says::Complex { .. }) => Gathered::Blocked,
                        None => {
                            let mut finest = level;
                            let mut constituents = Vec::new();
                            let (mut blocked, mut disqualified) = (false, false);
                            for child in region.children() {
                                let child_across = tiles_across(child.level);
                                match &cache[child.level][child.y * child_across + child.x] {
                                    Gathered::Whole { finest: child_finest, constituents: child_constituents } => {
                                        finest = finest.max(*child_finest);
                                        constituents.extend(child_constituents.iter().copied());
                                    }
                                    Gathered::Disqualified => disqualified = true,
                                    Gathered::Blocked => blocked = true,
                                }
                            }
                            // Every child is checked, whatever the
                            // first one finds: a `Blocked` child must
                            // never be hidden behind a `Disqualified`
                            // one.
                            if blocked {
                                Gathered::Blocked
                            } else if disqualified {
                                Gathered::Disqualified
                            } else {
                                Gathered::Whole { finest, constituents }
                            }
                        }
                    }
                });
            }
        }
        cache[level] = row;
    }
    cache
}

/// The deepest level any unobstructed `Bound` tile under `region`
/// reaches, or `None` if there is nothing absorbable under it at all.
/// A `Copied` or `Complex` tile contributes no depth requirement of its
/// own -- masked out, they need no resolution to be read back, so they
/// must never force one on everything around them the way an
/// unbounded, blindly-searched depth would. Computed once a round, the
/// same way [`precompute_gathered`] is: it depends only on `grid` and
/// `excluded`, fixed for the round, not on any candidate trying it.
fn precompute_natural_finest(grid: &[Vec<Option<Says>>], excluded: &[Vec<bool>]) -> Vec<Vec<Option<usize>>> {
    let mut cache: Vec<Vec<Option<usize>>> = (0..=CELL_LEVEL).map(|_| Vec::new()).collect();
    for level in (0..=CELL_LEVEL).rev() {
        let across = tiles_across(level);
        let mut row = Vec::with_capacity(across * across);
        for y in 0..across {
            for x in 0..across {
                let idx = y * across + x;
                row.push(if level == CELL_LEVEL || excluded[level][idx] {
                    None
                } else {
                    match &grid[level][idx] {
                        Some(Says::Bound(_)) => Some(level),
                        Some(_) => None, // Copied or Complex -- no depth requirement of its own
                        None => {
                            let region = Region { level, x, y };
                            region
                                .children()
                                .into_iter()
                                .filter_map(|child| {
                                    let child_across = tiles_across(child.level);
                                    cache[child.level][child.y * child_across + child.x]
                                })
                                .max()
                        }
                    }
                });
            }
        }
        cache[level] = row;
    }
    cache
}

/// Builds one [`MaskNode`] for `region`, given `limit_level` -- the
/// complex tile's own chosen tile size, the finest a mask is ever
/// allowed to go, and `gathered`, [`precompute_gathered`]'s own result
/// for every region this round. `region` itself being an existing
/// complex tile is checked first and is always fatal: nesting one
/// inside another's mask is forbidden, and it is one whole grid entry
/// with nothing finer beneath it to recurse into instead, so `None`
/// propagates straight out -- this `limit_level`, for the candidate
/// that reached here, is impossible. Otherwise: if the whole of
/// `region` gathers at `limit_level` or coarser, it is `Unmasked`,
/// decomposed into `limit_level`-sized tiles same as a plain complex
/// tile always has been. If `region` is already at `limit_level`
/// itself, there is nothing finer to try, so anything short of that is
/// final -- `Masked`, unless an existing complex tile was found
/// somewhere finer than `limit_level` inside it (`Gathered::Blocked`),
/// which is just as fatal here as finding one at `region` itself: a
/// masked area is read back by recursing into it at whatever depth is
/// actually there, not stopping at `limit_level`, so nesting is exactly
/// as forbidden three levels down as it is right here. Otherwise, the
/// same question is asked again of `region`'s own four children, at
/// half the size; if none of them managed anything either, `region` is
/// simply `Masked` whole, rather than a `Subdivided` of four `Masked`
/// children that would only cost more to say the same thing. Returns
/// the node and how many of `decide_tiles`' own tiles it absorbed --
/// zero for `Masked`.
fn build_mask_node(
    grid: &[Vec<Option<Says>>],
    gathered: &[Vec<Gathered>],
    region: Region,
    limit_level: usize,
) -> Option<(MaskNode, usize)> {
    let across = tiles_across(region.level);
    let idx = region.y * across + region.x;
    if let Some(Says::Complex { .. }) = &grid[region.level][idx] {
        return None;
    }

    let here = &gathered[region.level][idx];
    if let Gathered::Whole { finest, constituents } = here {
        if *finest <= limit_level {
            let values = flatten(region, limit_level, constituents);
            let n = constituents.len();
            return Some((MaskNode::Unmasked(values), n));
        }
    }
    if region.level == limit_level {
        return match here {
            Gathered::Blocked => None,
            _ => Some((MaskNode::Masked, 0)),
        };
    }
    let mut nodes: [Option<MaskNode>; 4] = [None, None, None, None];
    let (mut constituents, mut any_unmasked) = (0usize, false);
    for (i, child) in region.children().into_iter().enumerate() {
        let (node, node_constituents) = build_mask_node(grid, gathered, child, limit_level)?;
        any_unmasked |= !matches!(node, MaskNode::Masked);
        constituents += node_constituents;
        nodes[i] = Some(node);
    }
    if !any_unmasked {
        Some((MaskNode::Masked, 0)) // nothing reclaimed below -- cheaper to mask the whole of it
    } else {
        Some((MaskNode::Subdivided(Box::new(nodes.map(Option::unwrap))), constituents))
    }
}

/// Removes every tile a [`MaskNode::Unmasked`] absorbed from `grid`, so
/// the area composing them stops being visible as anything smaller. A
/// `Masked` area is left untouched -- it was never gathered into
/// anything -- and a `Subdivided` one recurses into its own children.
fn consume_mask_tree(grid: &mut [Vec<Option<Says>>], region: Region, node: &MaskNode) {
    match node {
        MaskNode::Unmasked(_) => consume(grid, region),
        MaskNode::Masked => {}
        MaskNode::Subdivided(children) => {
            for (child, node) in region.children().into_iter().zip(children.iter()) {
                consume_mask_tree(grid, child, node);
            }
        }
    }
}

/// Removes every tile [`gather`] just confirmed from `grid`. Follows
/// exactly the path `gather` found, so it never reaches a region
/// `gather` did not already accept.
fn consume(grid: &mut [Vec<Option<Says>>], region: Region) {
    let across = tiles_across(region.level);
    if grid[region.level][region.y * across + region.x].take().is_some() {
        return;
    }
    for child in region.children() {
        consume(grid, child);
    }
}

/// Marks every `Masked` area of `node`'s own subtree, and everything
/// below it down to cells, forever excluded from composing into a
/// complex tile of its own -- called once, when the mask tree naming
/// it is committed, never undone. An `Unmasked` area needs nothing:
/// [`consume_mask_tree`] already cleared it to `None`, and `gather`
/// never succeeds over a `None` subtree with nothing left inside it.
fn exclude_mask_tree(excluded: &mut [Vec<bool>], region: Region, node: &MaskNode) {
    match node {
        MaskNode::Unmasked(_) => {}
        MaskNode::Masked => exclude_subtree(excluded, region),
        MaskNode::Subdivided(children) => {
            for (child, node) in region.children().into_iter().zip(children.iter()) {
                exclude_mask_tree(excluded, child, node);
            }
        }
    }
}

/// Marks `region` and its whole subtree, down to cells, excluded.
fn exclude_subtree(excluded: &mut [Vec<bool>], region: Region) {
    let across = tiles_across(region.level);
    excluded[region.level][region.y * across + region.x] = true;
    if region.level < CELL_LEVEL {
        for child in region.children() {
            exclude_subtree(excluded, child);
        }
    }
}

/// Lays `constituents` out at `finest`, in the reading order
/// [`Region::tiles_at_depth`] reads a resolution back in -- a
/// constituent coarser than `finest` fills every one of its own
/// positions at that resolution with the same repeated value.
fn flatten(region: Region, finest: usize, constituents: &[(Region, bool)]) -> Vec<bool> {
    let side = 1usize << (finest - region.level);
    let mut values = vec![None; side * side];
    for &(constituent, value) in constituents {
        for tile in constituent.tiles_at_depth(finest - constituent.level) {
            let (local_x, local_y) = (tile.x - region.x * side, tile.y - region.y * side);
            values[local_y * side + local_x] = Some(value);
        }
    }
    values.into_iter().map(|value| value.expect("every position covered by exactly one constituent")).collect()
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

/// Runs the greedy pass and just counts what it placed, by size and
/// kind -- what [`super::greedy_tiles::run`] compares against dsrn.
pub fn greedy_tile_pass(pyramid: &Pyramid, bitmap: &Bitmap) -> GreedyTileCounts {
    let mut counts = GreedyTileCounts::default();
    for tile in decide_tiles(pyramid, bitmap) {
        match tile.says {
            Says::Bound(_) => counts.bound_at_level[tile.region.level] += 1,
            Says::Copied { .. } => counts.copied_at_level[tile.region.level] += 1,
            Says::Complex { .. } => counts.complex_at_level[tile.region.level] += 1,
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

/// Compares the greedy pass against the region-based encoder at its
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
        let (mut greedy_bound, mut greedy_copied, mut greedy_complex) =
            ([0usize; CELL_LEVEL + 1], [0usize; CELL_LEVEL + 1], [0usize; CELL_LEVEL + 1]);

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
                greedy_complex[level] += greedy.complex_at_level[level];
            }
        }

        let n = maps.len();
        println!("\n  {family}, {n} bitmaps. tiles a bitmap, by size.\n");
        let mut t = Table::new(&[
            "tile side\nin cells",
            "dsrn\nbound",
            "dsrn\ncopied",
            "greedy\nbound",
            "greedy\ncopied",
            "greedy\ncomplex",
        ]);
        let (mut dsrn_total, mut greedy_total) = (0usize, 0usize);
        for level in 0..=CELL_LEVEL {
            let side = crate::pyramid::tile_side(level);
            t.row(&[
                format!("{side}x{side}"),
                (dsrn_bound[level] / n).to_string(),
                (dsrn_copied[level] / n).to_string(),
                (greedy_bound[level] / n).to_string(),
                (greedy_copied[level] / n).to_string(),
                (greedy_complex[level] / n).to_string(),
            ]);
            dsrn_total += dsrn_bound[level] + dsrn_copied[level];
            greedy_total += greedy_bound[level] + greedy_copied[level] + greedy_complex[level];
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

