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
    /// `depth` levels below this region, every tile is homogeneous --
    /// not necessarily the same value as its neighbours, or this
    /// region would have been one `Bound` tile itself -- and `values`
    /// is every one of them, in reading order. No masking: every tile
    /// at `depth` has to be homogeneous for this to count at all.
    Complex { depth: usize, values: Vec<bool> },
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

/// Groups [`decide_tiles`]'s own placed tiles into complex tiles where
/// it can: wherever a tile-aligned area's four immediate children were
/// each placed as their own `Bound` tile -- not necessarily to the
/// same value, or the area would already be one `Bound` tile of its
/// own -- the four are replaced by one `Complex` tile covering all of
/// them.
///
/// A second pass over what the greedy pass already found, not a third
/// thing the pass itself decides: every group this composes is four
/// tiles decide_tiles already placed and verified on its own, so this
/// can never claim something is homogeneous that is not. Only `Bound`
/// children compose -- a `Copied` or already-`Complex` child does not,
/// which is what keeps this to one level and stops a composed tile
/// from ever being composed again into a coarser one. No masking
/// either: any one of the four not being `Bound` fails the whole
/// group, whatever the other three look like.
///
/// Never composes a tile whose children sit at [`CELL_LEVEL`] - 1 or
/// [`CELL_LEVEL`] - 2. [`super::tile_or_subdivide`]'s tree already
/// says a homogeneous one-above-cells region in two bits, cheaper than
/// any header naming a resolution could; grouping four of those into
/// one complex tile would cost 10 bits for what four such leaves and
/// their parent's subdivide bit already say in 9. Every coarser level
/// composes into a real saving: four ordinary three-bit leaves and the
/// subdivide bit above them cost 13, against 10 for one complex tile
/// naming them at once.
pub fn compose_complex_tiles(tiles: Vec<PlacedTile>) -> Vec<PlacedTile> {
    let mut grid: Vec<Vec<Option<Says>>> =
        (0..=CELL_LEVEL).map(|level| vec![None; tiles_in_level(level)]).collect();
    for PlacedTile { region, says } in tiles {
        let across = tiles_across(region.level);
        grid[region.level][region.y * across + region.x] = Some(says);
    }

    for parent_level in 0..CELL_LEVEL.saturating_sub(2) {
        let across = tiles_across(parent_level);
        for y in 0..across {
            for x in 0..across {
                let children = Region { level: parent_level, x, y }.children();
                let all_bound = children.iter().all(|child| {
                    let child_across = tiles_across(child.level);
                    matches!(
                        grid[child.level][child.y * child_across + child.x],
                        Some(Says::Bound(_))
                    )
                });
                if !all_bound {
                    continue;
                }
                let values = children
                    .iter()
                    .map(|child| {
                        let child_across = tiles_across(child.level);
                        match grid[child.level][child.y * child_across + child.x].take() {
                            Some(Says::Bound(value)) => value,
                            _ => unreachable!("just confirmed every child is Bound"),
                        }
                    })
                    .collect();
                grid[parent_level][y * across + x] = Some(Says::Complex { depth: 1, values });
            }
        }
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

