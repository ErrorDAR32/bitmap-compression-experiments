//! Building a tree out of the greedy pass's tile map, instead of
//! dsrn's own per-standing cost tables.
//!
//! Every fact this needs is already static and already in the
//! pyramid: whether a region is one thing, and whether some same-size
//! neighbour holds the same cells. Bottom-up, from the finest grammar
//! level to the root, each region asks one question: am I cheaper as
//! one tile of my own, or as my four quarters' own best answers added
//! together? No standing, no masking, no absorbing part of a region
//! while describing the rest again -- just the two ways dsrn's
//! grammar can say "the whole of me", compared against subdividing.

use crate::dsrn::nesting_data::{CODE_WIDTH, DIRECTION_WIDTH};
use crate::dsrn::region::Region;
use crate::pyramid::{tile_of_bitmap, tiles_across, Pyramid, CELL_LEVEL};
use crate::Bitmap;

/// What one region costs, and how it got there.
#[derive(Clone, Copy)]
enum Says {
    OwnTile,
    Subdivide,
}

/// The cheapest cost of every region, indexed the same way the
/// pyramid indexes tiles.
pub struct TileTree {
    cost: Vec<Vec<usize>>,
    says: Vec<Vec<Says>>,
}

impl TileTree {
    fn at(region: Region) -> usize {
        region.y * tiles_across(region.level) + region.x
    }

    pub fn cost_of(&self, region: Region) -> usize {
        self.cost[region.level][Self::at(region)]
    }
}

/// Builds the tree bottom-up: cells first, the whole bitmap last.
pub fn build(pyramid: &Pyramid, bitmap: &Bitmap) -> TileTree {
    let mut tree = TileTree {
        cost: (0..=CELL_LEVEL).map(|level| vec![0; tiles_across(level) * tiles_across(level)]).collect(),
        says: (0..=CELL_LEVEL)
            .map(|level| vec![Says::OwnTile; tiles_across(level) * tiles_across(level)])
            .collect(),
    };
    for level in (0..=CELL_LEVEL).rev() {
        let across = tiles_across(level);
        for y in 0..across {
            for x in 0..across {
                price(&mut tree, pyramid, bitmap, Region { level, x, y });
            }
        }
    }
    tree
}

/// What one region costs: itself, or its four quarters.
fn price(tree: &mut TileTree, pyramid: &Pyramid, bitmap: &Bitmap, region: Region) {
    let own_tile = if tile_of_bitmap(pyramid, bitmap, region.level, region.x, region.y).is_some() {
        Some(CODE_WIDTH + 1)
    } else if region.level < CELL_LEVEL && pyramid.copyable(region.level, region.x, region.y) {
        Some(CODE_WIDTH + DIRECTION_WIDTH)
    } else {
        None
    };

    let subdivide = (region.level < CELL_LEVEL).then(|| {
        CODE_WIDTH + region.children().into_iter().map(|child| tree.cost_of(child)).sum::<usize>()
    });

    let (cost, says) = match (own_tile, subdivide) {
        (Some(a), Some(b)) if a <= b => (a, Says::OwnTile),
        (Some(a), None) => (a, Says::OwnTile),
        (_, Some(b)) => (b, Says::Subdivide),
        (None, None) => unreachable!("a cell is always one thing"),
    };
    let at = TileTree::at(region);
    tree.cost[region.level][at] = cost;
    tree.says[region.level][at] = says;
}

/// Compares the tree built from the tile map against dsrn's own
/// exact result, in bits.
pub fn run() {
    use crate::dsrn::region::Region;
    use crate::dsrn::{encode, FourByFour, Knobs, Masking, Workspace};
    use crate::samples;

    let knobs = Knobs { masking: Masking::Anywhere, four_by_four: FourByFour::ItsOwnGrammar };
    let (mut pyramid, mut work, mut out) =
        (Pyramid::new(), Workspace::new(), crate::dsrn::Encoded::default());

    for (family, maps) in samples::every_family() {
        let (mut dsrn_bits, mut tree_bits) = (0usize, 0usize);
        for bitmap in &maps {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            encode(&pyramid, bitmap, knobs, &mut work, &mut out);
            dsrn_bits += out.bits();
            tree_bits += build(&pyramid, bitmap).cost_of(Region::whole_bitmap());
        }
        let n = maps.len();
        println!(
            "\n  {family}, {n} bitmaps: dsrn {} bits a bitmap, tile tree {} ({:+.1}%)",
            dsrn_bits / n,
            tree_bits / n,
            100.0 * (tree_bits as f64 - dsrn_bits as f64) / dsrn_bits as f64
        );
    }
}
