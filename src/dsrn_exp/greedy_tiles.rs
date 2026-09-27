//! An alternate algorithm to DSRN's own: a flat, size-ordered greedy
//! tile placement, with no region tree at all -- for [`decide_tiles`]
//! itself, at least.
//!
//! DSRN decides a region's whole subtree at once, comparing the exact
//! cost of every way it could go before committing to any of it.
//! [`decide_tiles`] is the opposite kind of pass over the bitmap
//! itself: there are no regions, no standing, no recursive cost
//! tables. There is a plane and a size, biggest first, and one rule at
//! each size: a tile that is one thing, or that holds the same cells
//! as a same-size neighbour, gets placed and claimed; a tile that is
//! neither is left for the next, finer size to try on its own four
//! quarters. Both checks read the bitmap directly and answer at once
//! -- the bitmap never changes, so there is nothing for either one to
//! wait on, whatever order tiles get visited in. Tiles never overlap,
//! so a size that has already claimed a tile is a size no finer pass
//! ever has to look at again, and by the time the pass reaches 1x1
//! every remaining cell is, on its own, one thing -- so the pass
//! always finishes and always covers the whole bitmap.
//!
//! [`compose_complex_tiles`], the pass after it, is not that kind of
//! pass at all: once `decide_tiles` has placed its tiles, working out
//! which of them are cheaper grouped into a complex tile is exactly
//! the small, bounded version of DSRN's own exact cost comparison --
//! see [`compute`]'s own doc comment.
//!
//! This file only decides which tiles get placed, and how they group.
//! It does not write a bitstream: there is nothing here yet that says
//! how a tile's own size, position and kind (bound to a value,
//! copying a direction, or a complex tile's own subtree) would be
//! spelled out in bits -- that is [`super::tile_or_subdivide`]'s own
//! job, working from exactly what this file decided.

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
    /// This region's own four children, in [`Region::children`] order,
    /// each its own [`Node`] -- see there for what one says.
    Complex(Box<[Node; 4]>),
}

/// One node of a complex tile's own subtree, over the area it covers.
///
/// This is not a separate idea from the complex tile itself -- it is
/// the complex tile's own subtree, one level of it, asked again and
/// again. `Leaf` and `Masked` both end it there; `Subdivided` asks the
/// same question of the area's own four children, at half the size,
/// all the way down to individual cells if that turns out cheapest.
/// There is no shared resolution to decompose to, and nothing here
/// ever names one: each `Leaf` covers exactly the area it turned out
/// to need, whatever that is, so nothing is ever repeated the way a
/// single resolution shared across a whole complex tile used to force
/// a coarser constituent to be.
#[derive(Clone)]
pub enum Node {
    /// This whole area is one value, contributed to the complex tile.
    Leaf(bool),
    /// This whole area is excluded, read as a plain region of its own
    /// -- nesting a complex tile here is forbidden, so it can only
    /// ever turn out `Bound`, `Copied`, or plain subdivision.
    Masked,
    /// The same question asked again of this area's own four children.
    Subdivided(Box<[Node; 4]>),
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

/// Groups [`decide_tiles`]'s own placed tiles into complex tiles,
/// wherever [`compute`] finds one cheaper than leaving the area to
/// ordinary subdivision -- see its own doc comment for the exact
/// method, an exact bottom-up cost comparison rather than a search.
/// This is a thin wrapper: build the grid `compute` walks, run it once
/// from the whole bitmap down, and read its decision back into the
/// same flat `Vec<PlacedTile>` shape every other pass in this module
/// uses.
pub fn compose_complex_tiles(tiles: Vec<PlacedTile>) -> Vec<PlacedTile> {
    let mut grid: Vec<Vec<Option<Says>>> =
        (0..=CELL_LEVEL).map(|level| vec![None; tiles_in_level(level)]).collect();
    for PlacedTile { region, says } in tiles {
        let across = tiles_across(region.level);
        grid[region.level][region.y * across + region.x] = Some(says);
    }
    let computed = compute(&grid, Region::whole_bitmap());
    let mut out = Vec::new();
    collect(Region::whole_bitmap(), computed, &mut out);
    out
}

/// Bit widths shared with [`super::tile_or_subdivide`]'s own grammar --
/// kept here because deciding whether a choice is cheaper needs the
/// same numbers the encoder actually spends on it. `tile_or_subdivide`
/// imports these rather than repeating them, so the two can never
/// drift apart from each other.
pub(crate) const LEAF_WIDTH: usize = 1;
pub(crate) const CODE_WIDTH: usize = 1;
pub(crate) const COMPLEX_FLAG_WIDTH: usize = 1;
pub(crate) const FAR_WIDTH: usize = 1;
pub(crate) const DIRECTION_WIDTH: usize = 2;
pub(crate) const VALUE_WIDTH: usize = 1;
pub(crate) const NODE_LEAF_WIDTH: usize = 1;
pub(crate) const NODE_STATE_WIDTH: usize = 1;

/// An ordinary `Bound` leaf, outside any mask: leaf, code, the
/// complex-flag bit every bind pays, and its value.
const ORDINARY_BOUND_COST: usize = LEAF_WIDTH + CODE_WIDTH + COMPLEX_FLAG_WIDTH + VALUE_WIDTH;
/// The same leaf inside a mask, where nesting a complex tile is
/// forbidden and the complex-flag bit has nothing left to distinguish.
const MASKED_BOUND_COST: usize = LEAF_WIDTH + CODE_WIDTH + VALUE_WIDTH;
/// A `Copied` leaf: leaf, code, far/near, direction -- the same either
/// side of a mask, since a copy never had a complex-flag bit to skip.
const COPY_COST: usize = LEAF_WIDTH + CODE_WIDTH + FAR_WIDTH + DIRECTION_WIDTH;
/// One above cells, a homogeneous 2x2's own special-cased leaf bit and
/// value -- see `tile_or_subdivide`'s own doc comment for why this
/// level never gets the general grammar.
const TWO_BY_TWO_HOMOGENEOUS_COST: usize = LEAF_WIDTH + VALUE_WIDTH;
/// The same 2x2, not a whole `Bound` tile (whether it is `Copied` or
/// still subdivided makes no difference -- see [`compute`]): one leaf
/// bit, then its four cells written out raw once the whole tree is
/// read.
const TWO_BY_TWO_HOLE_COST: usize = LEAF_WIDTH + 4;
/// A complex tile's own header: leaf, code, the complex flag. No
/// resolution field and no mask-present bit any more -- a [`Node`]'s
/// own subdivide/leaf/mask bits say everything else there is to say.
const COMPLEX_HEADER_COST: usize = LEAF_WIDTH + CODE_WIDTH + COMPLEX_FLAG_WIDTH;
/// A [`Node::Subdivided`]'s own bit.
const NODE_SUBDIVIDE_COST: usize = NODE_LEAF_WIDTH;
/// A [`Node::Leaf`]'s own cost: leaf bit, masked/unmasked bit, value.
const NODE_LEAF_COST: usize = NODE_LEAF_WIDTH + NODE_STATE_WIDTH + VALUE_WIDTH;
/// What a [`Node::Masked`] pays before its delegate's own cost: leaf
/// bit, masked/unmasked bit.
const NODE_MASKED_TAX: usize = NODE_LEAF_WIDTH + NODE_STATE_WIDTH;

/// Everything [`compute`] works out for one region: what it costs
/// several different ways, and the actual choices those costs settle.
struct Computed {
    /// This region's own placement in the ordinary tree -- `None` when
    /// ordinary subdivision is cheaper than anything this region could
    /// say for itself.
    says: Option<Says>,
    /// What `says` costs -- or, when `says` is `None`, what
    /// subdividing into `children` costs -- with a descendant still
    /// free to be its own complex tile.
    cost: usize,
    /// The same region's cost with a complex tile forbidden anywhere
    /// under it -- what a [`Node::Masked`] elsewhere pays to delegate
    /// to this region.
    plain_cost: usize,
    /// The cheapest way to fold this region into a complex tile's own
    /// subtree instead.
    node: Node,
    /// What `node` costs.
    node_cost: usize,
    /// This region's own four children, already worked out -- kept so
    /// that, if ordinary subdivision wins, they need not be worked out
    /// a second time to build the final tile list. `None` for a
    /// terminal region (one placed tile, or a hole one above cells).
    children: Option<Box<[Computed; 4]>>,
}

/// Works out, bottom-up, the cheapest way to encode every region under
/// (and including) `region`, in both senses [`Computed`] tracks: as
/// itself, in the ordinary tree, and as one [`Node`] of some ancestor's
/// complex tile. A region's own answer needs its children's first, so
/// this recurses into them before it decides anything about itself --
/// the same order [`crate::dsrn::coarsest::coarsest`] prices dsrn's own
/// regions in, just over a far smaller set of choices: a region here is
/// either exactly what [`decide_tiles`] already placed, or, if that
/// left it subdivided, either plain subdivision or one complex tile
/// covering the whole of it -- never a resolution or a tile size to
/// search, since a [`Node`] pays for exactly the area it turns out to
/// need and nothing forces it to repeat a coarser constituent the way
/// a resolution shared across the whole tile once did.
///
/// A complex tile is chosen exactly when its header plus its four
/// children's own [`Node`] cost beats subdividing plainly and letting
/// each child be its own best self (own tile, own complex tile, or
/// further subdivision) -- real bit costs compared directly, not a
/// ratio standing in for them. Nesting is forbidden by construction
/// here, not by a check: a [`Node::Masked`] delegates to `plain_cost`,
/// which forbids a complex tile anywhere under it, all the way down,
/// exactly mirroring `complex_allowed = false`'s own reach through
/// `tile_or_subdivide::encode_region`.
fn compute(grid: &[Vec<Option<Says>>], region: Region) -> Computed {
    if region.level == CELL_LEVEL {
        // A cell is always one value, and nothing above ever benefits
        // from asking whether to mask it: excluding it would cost more
        // (its own two bits of overhead) than simply naming it ever
        // could.
        let across = tiles_across(region.level);
        let Some(Says::Bound(value)) = grid[region.level][region.y * across + region.x] else {
            unreachable!("decide_tiles places every cell as its own whole tile");
        };
        return Computed { says: None, cost: 0, plain_cost: 0, node: Node::Leaf(value), node_cost: 1, children: None };
    }

    let across = tiles_across(region.level);
    let idx = region.y * across + region.x;

    if region.level == CELL_LEVEL - 1 {
        // One above cells: the tree's own special case, which only
        // ever reads a whole `Bound` 2x2 -- a `Copied` one is silently
        // treated the same as a still-subdivided one, a hole left to
        // the trailing raw pass, exactly as `tile_or_subdivide`'s own
        // `encode_region` does. A `Node`'s own leaf/subdivide bits
        // share none of that restriction, so a complex tile can still
        // recurse past this level into individual cells if that turns
        // out cheaper than treating the 2x2 as one node.
        let (says, outer_cost, leaf, subdivided) = match &grid[region.level][idx] {
            Some(Says::Bound(value)) => (
                Some(Says::Bound(*value)),
                TWO_BY_TWO_HOMOGENEOUS_COST,
                Some((Node::Leaf(*value), NODE_LEAF_COST)),
                None, // one whole placed tile -- decide_tiles never placed anything finer under it
            ),
            Some(Says::Copied { .. }) => (
                None,
                TWO_BY_TWO_HOLE_COST,
                None, // not one value, so no plain `Leaf`
                None, // one whole placed tile -- same as `Bound`, nothing finer exists under it
            ),
            Some(Says::Complex(_)) => unreachable!("decide_tiles never places a complex tile"),
            None => {
                // Genuinely left to its own four cells -- each of them
                // has its own grid entry to recurse into, unlike a
                // whole `Bound` or `Copied` 2x2's children, which
                // decide_tiles never populated at all.
                let kids = region.children();
                let children: [Computed; 4] = std::array::from_fn(|i| compute(grid, kids[i]));
                (
                    None,
                    TWO_BY_TWO_HOLE_COST,
                    None,
                    Some((
                        Node::Subdivided(Box::new(std::array::from_fn::<Node, 4, _>(|i| children[i].node.clone()))),
                        NODE_SUBDIVIDE_COST + children.iter().map(|c| c.node_cost).sum::<usize>(),
                    )),
                )
            }
        };
        let masked = (Node::Masked, NODE_MASKED_TAX + outer_cost);
        let (node, node_cost) = [leaf, Some(masked), subdivided]
            .into_iter()
            .flatten()
            .min_by_key(|(_, cost)| *cost)
            .expect("masked is always a candidate");
        return Computed { says, cost: outer_cost, plain_cost: outer_cost, node, node_cost, children: None };
    }

    match &grid[region.level][idx] {
        Some(Says::Bound(value)) => Computed {
            says: Some(Says::Bound(*value)),
            cost: ORDINARY_BOUND_COST,
            plain_cost: MASKED_BOUND_COST,
            node: Node::Leaf(*value),
            node_cost: NODE_LEAF_COST,
            children: None,
        },
        Some(Says::Copied { far, direction }) => Computed {
            says: Some(Says::Copied { far: *far, direction: *direction }),
            cost: COPY_COST,
            plain_cost: COPY_COST,
            node: Node::Masked,
            node_cost: NODE_MASKED_TAX + COPY_COST,
            children: None,
        },
        Some(Says::Complex(_)) => unreachable!("decide_tiles never places a complex tile"),
        None => {
            let kids = region.children();
            let children: [Computed; 4] = std::array::from_fn(|i| compute(grid, kids[i]));

            let subdivide_cost = LEAF_WIDTH + children.iter().map(|c| c.cost).sum::<usize>();
            let complex_cost = COMPLEX_HEADER_COST + children.iter().map(|c| c.node_cost).sum::<usize>();
            let plain_cost = LEAF_WIDTH + children.iter().map(|c| c.plain_cost).sum::<usize>();

            let (says, cost) = if complex_cost < subdivide_cost {
                let nodes: [Node; 4] = std::array::from_fn(|i| children[i].node.clone());
                (Some(Says::Complex(Box::new(nodes))), complex_cost)
            } else {
                (None, subdivide_cost)
            };

            let node_subdivide_cost = NODE_SUBDIVIDE_COST + children.iter().map(|c| c.node_cost).sum::<usize>();
            let node_masked_cost = NODE_MASKED_TAX + plain_cost;
            let (node, node_cost) = if node_subdivide_cost <= node_masked_cost {
                let nodes: [Node; 4] = std::array::from_fn(|i| children[i].node.clone());
                (Node::Subdivided(Box::new(nodes)), node_subdivide_cost)
            } else {
                (Node::Masked, node_masked_cost)
            };

            Computed { says, cost, plain_cost, node, node_cost, children: Some(Box::new(children)) }
        }
    }
}

/// Walks [`compute`]'s own decision back down into a flat tile list: a
/// region with a placed `says` is one tile (a complex tile's whole
/// subtree included, via its own [`Node`] tree -- nothing under it
/// gets its own entry here); a region left `None` is read off its own
/// `children` instead, which `compute` already worked out and kept
/// rather than needing them worked out a second time. A `None` with no
/// `children` at all is a 2x2 (or finer) hole, left entirely to the
/// trailing raw pass -- there is nothing here for it to say.
fn collect(region: Region, computed: Computed, out: &mut Vec<PlacedTile>) {
    match computed.says {
        Some(says) => out.push(PlacedTile { region, says }),
        None => {
            if let Some(children) = computed.children {
                for (child, child_computed) in region.children().into_iter().zip(*children) {
                    collect(child, child_computed, out);
                }
            }
        }
    }
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
            Says::Complex(_) => counts.complex_at_level[tile.region.level] += 1,
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
