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
    /// `Unmasked` area of `mask` settles on -- one `MaskNode` for each
    /// of this region's own four children, in [`Region::children`]
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
/// than 2x2 -- except wherever one of the area's own four direct
/// children is masked out, which excludes that whole child instead of
/// disqualifying the rest -- said once at the coarsest resolution that
/// still covers every one of its unmasked tiles, the smallest of their
/// own sizes. A tile bigger than that resolution decomposes into that
/// many repeats of its own value; nothing is ever cut, since every
/// placed tile already lines up with the same power-of-two grid a
/// candidate area does, so it is always either wholly inside that area
/// or wholly outside it, never straddling the edge. A masked child is
/// left exactly as it stood before this tile composed, and forever
/// excluded from composing into a complex tile of its own -- no
/// nesting, at least for a first version of this -- so it can only
/// ever turn out `Bound`, `Copied`, or plain subdivision, read as a
/// region of its own wherever this tile is written out. A `Copied`
/// tile or a 1x1 anywhere still un-masked disqualifies the whole area,
/// whatever the rest of it looks like. Only `Bound` tiles ever
/// compose, so a tile this pass just placed -- `Complex` -- never
/// composes again into a coarser one, masked or not.
///
/// The naive greedy search this settled on, rather than biggest area
/// first with no comparison: every valid area, of every size and
/// position, is tried at every resolution it could possibly settle on,
/// coarsest tile size to finest, and the round commits exactly one --
/// the one absorbing the most of `decide_tiles`' own tiles for the
/// fewest tiles its payload ends up naming, `constituents / payload`,
/// breaking a tie toward the larger area. For a given resolution, each
/// of the area's own four children is asked, top down: does it gather
/// whole at this resolution or coarser ([`build_mask_node`])? If so,
/// `Unmasked`. If not, ask the same of its own four children instead,
/// at half the size -- and so on, down to the resolution itself, where
/// a tile that still does not gather is finally `Masked` and left
/// alone. `payload` is a count of tiles, not a count of bits: how many
/// tiles at the resolution this area settles on it takes to cover its
/// *unmasked* footprint, one value bit each -- a masked area is never
/// counted, on either side of the ratio. That ratio is 1.0 exactly when
/// every one of an area's constituents is already sized to that
/// resolution -- nothing decomposed, nothing repeated -- and falls the
/// further below it the more a bigger constituent's single value gets
/// repeated across several payload tiles for nothing a coarser
/// resolution would have had to repeat at all -- which is exactly what
/// favouring the ratio closest to one avoids: the failure this
/// replaced, where the single biggest area that merely *qualified*
/// could drag an otherwise-uniform region down to whatever resolution
/// its one smallest tile demanded. Masking whatever does not gather is
/// the same fix applied at every finer size in turn, rather than only
/// once at the top: trying every resolution and letting the ratio pick
/// among them is what decides whether excluding something is actually
/// worth the area lost. Picking one candidate can only remove others --
/// an area it just absorbed cannot be gathered into anything else, and
/// neither can whatever it left masked, forever -- never add one, so
/// re-scanning every candidate from scratch each round is wasteful but
/// never wrong, and the round after nothing qualifies is where this
/// stops.
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
    // cells -- set the moment [`build_mask_node`] settles on `Masked`
    // for it, and never cleared, so no later round can compose any of
    // it into a complex tile of its own. No nesting, at least for a
    // first version of this: a masked area only ever gives part of
    // itself back to the one complex tile that masked it in the first
    // place, by unmasking one of its own children in turn -- never by
    // becoming a whole new complex tile of its own.
    let mut excluded: Vec<Vec<bool>> = (0..=CELL_LEVEL).map(|level| vec![false; tiles_in_level(level)]).collect();

    // Every possible complex tile, tried again from scratch each round:
    // committing one can only ever remove candidates (an area it just
    // absorbed can no longer be gathered into anything else), never add
    // one, so re-scanning is wasteful but never wrong.
    loop {
        let mut best: Option<(f64, usize, Region, usize, [MaskNode; 4])> = None;
        for level in 0..=(CELL_LEVEL - 2) {
            let across = tiles_across(level);
            for y in 0..across {
                for x in 0..across {
                    if grid[level][y * across + x].is_some() || excluded[level][y * across + x] {
                        // Already one placed tile, a complex tile from
                        // an earlier round, or forever excluded by an
                        // ancestor complex tile's own mask -- nothing
                        // left here for a coarser one to find.
                        continue;
                    }
                    let region = Region { level, x, y };
                    // Every resolution this area could settle on,
                    // coarsest (its own four children) to finest (its
                    // own cells).
                    for depth in 1..=(CELL_LEVEL - level) {
                        let limit_level = level + depth;
                        let mut nodes: [Option<MaskNode>; 4] = [None, None, None, None];
                        let (mut constituents, mut payload) = (0usize, 0usize);
                        for (i, child) in region.children().into_iter().enumerate() {
                            let (node, node_constituents, node_payload) =
                                build_mask_node(&grid, child, limit_level);
                            constituents += node_constituents;
                            payload += node_payload;
                            nodes[i] = Some(node);
                        }
                        if constituents == 0 {
                            continue; // nothing gained at this resolution at all
                        }
                        let ratio = constituents as f64 / payload as f64;
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

/// Whether every placed tile under `region` is `Bound` and none of
/// them finer than 2x2 -- and if so, the finest level among them and
/// every one of them, region and value, in no particular order.
/// `None` the moment a `Copied` tile or a 1x1 turns up anywhere in the
/// subtree, without touching `grid` at all: this only looks, it never
/// commits to anything until the whole area has been confirmed.
fn gather(grid: &[Vec<Option<Says>>], region: Region) -> Option<(usize, Vec<(Region, bool)>)> {
    if region.level == CELL_LEVEL {
        return None; // a 1x1 tile never takes part in a complex tile
    }
    let across = tiles_across(region.level);
    match grid[region.level][region.y * across + region.x] {
        Some(Says::Bound(value)) => Some((region.level, vec![(region, value)])),
        Some(_) => None, // Copied, or already Complex -- disqualified
        None => {
            let mut finest = region.level;
            let mut constituents = Vec::new();
            for child in region.children() {
                let (child_finest, mut child_constituents) = gather(grid, child)?;
                finest = finest.max(child_finest);
                constituents.append(&mut child_constituents);
            }
            Some((finest, constituents))
        }
    }
}

/// Builds one [`MaskNode`] for `region`, given `limit_level` -- the
/// complex tile's own chosen tile size, the finest a mask is ever
/// allowed to go. Tries [`gather`] first: if the whole of `region`
/// gathers at `limit_level` or coarser, it is `Unmasked`, decomposed
/// into `limit_level`-sized tiles same as a plain complex tile always
/// has been. If `region` is already at `limit_level` itself, there is
/// nothing finer to try, so a `gather` failure there is final --
/// `Masked`. Otherwise, the same question is asked again of `region`'s
/// own four children, at half the size; if none of them managed
/// anything either, `region` is simply `Masked` whole, rather than a
/// `Subdivided` of four `Masked` children that would only cost more to
/// say the same thing. Returns the node, how many of `decide_tiles`'
/// own tiles it absorbed, and how many `limit_level` payload tiles
/// that cost -- both zero for `Masked`.
fn build_mask_node(grid: &[Vec<Option<Says>>], region: Region, limit_level: usize) -> (MaskNode, usize, usize) {
    if let Some((finest, constituents)) = gather(grid, region) {
        if finest <= limit_level {
            let n = constituents.len();
            let values = flatten(region, limit_level, &constituents);
            let payload = values.len();
            return (MaskNode::Unmasked(values), n, payload);
        }
    }
    if region.level == limit_level {
        return (MaskNode::Masked, 0, 0);
    }
    let mut nodes: [Option<MaskNode>; 4] = [None, None, None, None];
    let (mut constituents, mut payload, mut any_unmasked) = (0usize, 0usize, false);
    for (i, child) in region.children().into_iter().enumerate() {
        let (node, node_constituents, node_payload) = build_mask_node(grid, child, limit_level);
        any_unmasked |= !matches!(node, MaskNode::Masked);
        constituents += node_constituents;
        payload += node_payload;
        nodes[i] = Some(node);
    }
    if !any_unmasked {
        return (MaskNode::Masked, 0, 0); // nothing reclaimed below -- cheaper to mask the whole of it
    }
    (MaskNode::Subdivided(Box::new(nodes.map(Option::unwrap))), constituents, payload)
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

/// Removes every tile [`gather`] just confirmed from `grid`.  Follows
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

