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
    /// `depth` levels below this region, every tile not under a masked
    /// child is homogeneous -- not necessarily the same value as its
    /// neighbours, or this region would have been one `Bound` tile
    /// itself -- and `values` is every one of them, in reading order,
    /// skipping every tile a masked child's footprint covers. `mask`
    /// names which of this region's own four children (in
    /// [`Region::children`] order) are excluded entirely.
    ///
    /// A masked child is always, itself, one single `Bound` tile
    /// [`decide_tiles`] already placed there, coarser than this tile's
    /// own resolution -- the one thing masking is for, as a baseline:
    /// letting a large homogeneous tile the chosen resolution would
    /// otherwise have decomposed into many repeated payload values
    /// instead be read as the one plain leaf it already is, right where
    /// it sits. Left untouched, unconsumed -- `compose_complex_tiles`
    /// never masks anything else (a `Copied` tile, a still-subdivided
    /// area, or an existing complex tile), so a masked child can never
    /// itself need to say anything more than that one value, and never
    /// needs excluding from a later round either: its `grid` entry is
    /// still there, so nothing else can gather it in, and this tile's
    /// own entry, once placed, is what stops anything coarser from ever
    /// reaching in to look. `0` is no masking at all, the original
    /// behaviour.
    Complex { depth: usize, mask: u8, values: Vec<bool> },
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
/// children is masked out instead, said once at the coarsest resolution
/// that still covers every one of its unmasked tiles, the smallest of
/// their own sizes. A tile bigger than that resolution decomposes into
/// that many repeats of its own value; nothing is ever cut, since every
/// placed tile already lines up with the same power-of-two grid a
/// candidate area does, so it is always either wholly inside that area
/// or wholly outside it, never straddling the edge.
///
/// A masked child is always, itself, one whole `Bound` tile
/// `decide_tiles` already placed there -- nothing else is ever masked.
/// As a baseline, that is the one thing masking is for: a large
/// homogeneous tile the chosen resolution would otherwise have to
/// decompose into many repeated payload values, read instead as the one
/// plain leaf it already is. A `Copied` tile, a still-subdivided area,
/// or an existing complex tile anywhere still un-masked disqualifies
/// the whole area, exactly as if masking did not exist -- there is
/// nothing to route around them with, at least for a first version of
/// this. Only `Bound` tiles ever compose, so a tile this pass just
/// placed -- `Complex` -- never composes again into a coarser one.
///
/// Masking one whole `Bound` tile can never reach the trouble a more
/// general mask could: it is never `None` (there is nothing to
/// recurse into, so no later round can ever place a new complex tile
/// somewhere an existing one's mask will walk straight into when
/// finally read back), and never an existing complex tile (so nesting
/// one inside another's mask, which is forbidden, never arises). And
/// once this tile is placed, its own `grid` entry is what stops
/// anything coarser from ever reaching in to look at what it masked --
/// the same protection an unmasked `Bound` tile already had.
///
/// The naive greedy search this settled on, rather than biggest area
/// first with no comparison: every valid area, of every size, position
/// and masking of its own four children, is a candidate every round --
/// masking is genuinely brute forced, all sixteen ways to include or
/// exclude each of the four, since there is no cheaper way to know
/// which exclusion (if any) improves the ratio below -- and the round
/// commits exactly one -- the one absorbing the most of `decide_tiles`'
/// own tiles for the fewest tiles its payload ends up naming,
/// `constituents / payload`, breaking a tie toward the larger area.
/// `payload` is a count of tiles, not a count of bits: how many tiles
/// at the resolution the area settles on it takes to cover its
/// *unmasked* footprint, one value bit each -- a masked child's own
/// footprint is never counted, on either side of the ratio. That ratio
/// is 1.0 exactly when every one of an area's constituents is already
/// sized to that resolution -- nothing decomposed, nothing repeated --
/// and falls the further below it the more a bigger constituent's
/// single value gets repeated across several payload tiles for nothing
/// a coarser resolution would have had to repeat at all -- exactly the
/// case masking a large constituent out of the payload fixes, by taking
/// both its one constituent and its many repeated payload tiles out of
/// the ratio at once. Picking one candidate can only remove others -- an
/// area it just absorbed cannot be gathered into anything else, and
/// neither can whatever it left masked, since that tile's own `grid`
/// entry is untouched and still there -- never add one, so re-scanning
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

    // Every possible complex tile, tried again from scratch each round:
    // committing one can only ever remove candidates (an area it just
    // absorbed can no longer be gathered into anything else), never add
    // one, so re-scanning is wasteful but never wrong.
    loop {
        let mut best: Option<(f64, usize, Region, u8, usize, Vec<(Region, bool)>)> = None;
        for level in 0..=(CELL_LEVEL - 2) {
            let across = tiles_across(level);
            for y in 0..across {
                for x in 0..across {
                    if grid[level][y * across + x].is_some() {
                        // Already one placed tile, or a complex tile
                        // from an earlier round -- nothing left here
                        // for a coarser one to find.
                        continue;
                    }
                    let region = Region { level, x, y };
                    // Every way to mask the region's own four children,
                    // short of masking all four (nothing left to say).
                    for mask in 0u8..0b1111 {
                        let Some((finest, constituents)) = gather_masked(&grid, region, mask) else {
                            continue;
                        };
                        let side = 1usize << (finest - level);
                        let child_side = side / 2;
                        let masked = mask.count_ones() as usize;
                        // A count of tiles at the resolution this area
                        // would settle on, not a count of bits -- one
                        // value bit each, so the two happen to
                        // coincide -- over the unmasked footprint only.
                        let payload = side * side - masked * child_side * child_side;
                        // How many of decide_tiles' own tiles this
                        // absorbs against how many payload tiles it
                        // costs to say them -- 1.0 at its best, when
                        // every one of them is already at the
                        // resolution this settles on, and falling the
                        // further from it the more a bigger
                        // constituent's single value gets repeated
                        // across payload tiles a coarser resolution
                        // wouldn't have had to repeat at all.
                        let ratio = constituents.len() as f64 / payload as f64;
                        let area = region.side_in_cells() * region.side_in_cells();
                        let better = match best {
                            Some((best_ratio, best_area, ..)) => {
                                ratio > best_ratio || (ratio == best_ratio && area > best_area)
                            }
                            None => true,
                        };
                        if better {
                            best = Some((ratio, area, region, mask, finest, constituents));
                        }
                    }
                }
            }
        }
        let Some((_, _, region, mask, finest, constituents)) = best else { break };
        consume_masked(&mut grid, region, mask);
        let across = tiles_across(region.level);
        grid[region.level][region.y * across + region.x] = Some(Says::Complex {
            depth: finest - region.level,
            mask,
            values: flatten(region, finest, mask, &constituents),
        });
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

/// [`gather`], but over only the children `mask` does not name -- a
/// named child is skipped entirely, left for whatever it already was.
/// Requires every un-named child to still gather successfully: masking
/// only ever removes a child from consideration, it does not relax
/// what the rest of them have to be. `mask == 0` gathers all four,
/// exactly [`gather`] itself would over `region`.
///
/// A named child may only be one whole `Bound` tile of its own --
/// `decide_tiles`' own placement, read directly off `grid`, never
/// anything `gather` would have had to recurse to confirm. That is the
/// one thing masking is for, as a baseline (see `compose_complex_tiles`):
/// nothing else -- a `Copied` tile, a still-subdivided area, or an
/// existing complex tile -- is ever masked, so a masked child is always
/// a single, already-terminal leaf, never something reading it back
/// could mistake for more than that, and never something a later round
/// could place anything new inside of either.
fn gather_masked(grid: &[Vec<Option<Says>>], region: Region, mask: u8) -> Option<(usize, Vec<(Region, bool)>)> {
    let mut finest = region.level;
    let mut constituents = Vec::new();
    for (i, child) in region.children().into_iter().enumerate() {
        if mask & (1 << i) != 0 {
            let across = tiles_across(child.level);
            if !matches!(grid[child.level][child.y * across + child.x], Some(Says::Bound(_))) {
                return None; // only a whole existing Bound tile may be masked
            }
            continue;
        }
        let (child_finest, mut child_constituents) = gather(grid, child)?;
        finest = finest.max(child_finest);
        constituents.append(&mut child_constituents);
    }
    Some((finest, constituents))
}

/// Removes every tile [`gather`] just confirmed from `grid`, so the
/// region composing them stops being visible as anything smaller.
/// Follows exactly the path `gather` found, so it never reaches a
/// region `gather` did not already accept.
fn consume(grid: &mut [Vec<Option<Says>>], region: Region) {
    let across = tiles_across(region.level);
    if grid[region.level][region.y * across + region.x].take().is_some() {
        return;
    }
    for child in region.children() {
        consume(grid, child);
    }
}

/// [`consume`], but only over the children `mask` does not name -- a
/// masked child is a whole `Bound` tile of its own and is left exactly
/// as it was: still there for whatever reads this complex tile back to
/// find, and, being still there, still exactly what stops any later
/// round from ever placing something new in its place.
fn consume_masked(grid: &mut [Vec<Option<Says>>], region: Region, mask: u8) {
    for (i, child) in region.children().into_iter().enumerate() {
        if mask & (1 << i) == 0 {
            consume(grid, child);
        }
    }
}

/// Lays `constituents` out at `finest`, in the reading order
/// [`Region::tiles_at_depth`] reads a resolution back in, skipping
/// every tile that falls under one of `mask`'s named children -- a
/// constituent coarser than `finest` fills every one of its own
/// positions at that resolution with the same repeated value.
fn flatten(region: Region, finest: usize, mask: u8, constituents: &[(Region, bool)]) -> Vec<bool> {
    let side = 1usize << (finest - region.level);
    let mut values = vec![None; side * side];
    for &(constituent, value) in constituents {
        for tile in constituent.tiles_at_depth(finest - constituent.level) {
            let (local_x, local_y) = (tile.x - region.x * side, tile.y - region.y * side);
            values[local_y * side + local_x] = Some(value);
        }
    }
    let depth = finest - region.level;
    region
        .tiles_at_depth(depth)
        .into_iter()
        .enumerate()
        .filter(|&(_, tile)| mask & (1 << region.child_holding(depth, tile)) == 0)
        .map(|(i, _)| values[i].expect("every unmasked position covered by exactly one constituent"))
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

