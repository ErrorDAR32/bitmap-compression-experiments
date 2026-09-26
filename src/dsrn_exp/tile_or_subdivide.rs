//! Pairing the greedy pass with an actual quadtree.
//!
//! At every region, biggest first: try covering its inside with
//! homogeneous tiles bigger than a cell, and see what that leaves
//! over. Committing to that tiling costs one fixed header a tile;
//! whatever it leaves over costs one bit a cell and nothing else,
//! since there is no cheaper way to say a single bit than the bit
//! itself. If the tiling's own header cost beats the cell count it
//! leaves over, it wins and the region is done. If it doesn't, the
//! region gives up on describing itself at all, and its four children
//! each get the same choice, one level finer.
//!
//! This is the first piece of the scheme: homogeneous tiles only, no
//! copying yet.

use crate::dsrn::region::Region;
use crate::pyramid::{tile_of_bitmap, Pyramid, CELL_LEVEL};
use crate::Bitmap;

const SIZE_WIDTH: usize = 3;
const VALUE_WIDTH: usize = 1;

/// What a leaf costs to describe: a single cell costs its own bit and
/// nothing else; anything bigger needs to say its size too.
fn payload_cost(level: usize) -> usize {
    if level == CELL_LEVEL {
        1
    } else {
        SIZE_WIDTH + VALUE_WIDTH
    }
}

/// A region the tree settled on, bound to a value, whatever its size.
#[derive(Clone, Copy)]
pub struct Leaf {
    pub region: Region,
    pub value: bool,
}

/// Builds the tree top-down from `region` inward, pushing every leaf
/// it settles on into `out` and counting every region that gave up
/// and subdivided into `subdivisions`.
pub fn decide(pyramid: &Pyramid, bitmap: &Bitmap, region: Region, out: &mut Vec<Leaf>, subdivisions: &mut usize) {
    if let Some(value) = tile_of_bitmap(pyramid, bitmap, region.level, region.x, region.y) {
        out.push(Leaf { region, value });
        return;
    }

    let (tiles, leftover) = tile_the_inside(pyramid, bitmap, region);
    let payload: usize = tiles.iter().map(|tile| payload_cost(tile.region.level)).sum();

    if payload < leftover.len() {
        out.extend(tiles);
        out.extend(
            leftover
                .into_iter()
                .map(|cell| Leaf { region: cell, value: bitmap.get(cell.x as u8, cell.y as u8) }),
        );
    } else {
        *subdivisions += 1;
        for child in region.children() {
            decide(pyramid, bitmap, child, out, subdivisions);
        }
    }
}

/// Every homogeneous tile strictly inside `region` -- bigger than a
/// cell, smaller than the region itself -- biggest first, and every
/// cell none of them covered, as its own 1x1 region.
fn tile_the_inside(pyramid: &Pyramid, bitmap: &Bitmap, region: Region) -> (Vec<Leaf>, Vec<Region>) {
    let side = region.side_in_cells();
    let (rx, ry) = region.top_left_cell();
    let mut claimed = vec![false; side * side];
    let mut tiles = Vec::new();

    for depth in 1..(CELL_LEVEL - region.level) {
        for tile in region.tiles_at_depth(depth) {
            let (tx, ty) = tile.top_left_cell();
            let (lx, ly) = (tx - rx, ty - ry);
            if claimed[ly * side + lx] {
                continue;
            }
            let Some(value) = tile_of_bitmap(pyramid, bitmap, tile.level, tile.x, tile.y) else {
                continue;
            };
            let tile_side = tile.side_in_cells();
            for row in 0..tile_side {
                for col in 0..tile_side {
                    claimed[(ly + row) * side + (lx + col)] = true;
                }
            }
            tiles.push(Leaf { region: tile, value });
        }
    }

    let mut leftover = Vec::new();
    for row in 0..side {
        for col in 0..side {
            if !claimed[row * side + col] {
                leftover.push(Region { level: CELL_LEVEL, x: rx + col, y: ry + row });
            }
        }
    }
    (tiles, leftover)
}

/// Node count and payload bits against dsrn's own, in the same
/// currency [`crate::dsrn_exp::greedy_tiles`] already compares in.
pub fn run() {
    use crate::dsrn::{encode, FourByFour, Knobs, Masking, Workspace};
    use crate::samples;

    let knobs = Knobs { masking: Masking::Anywhere, four_by_four: FourByFour::ItsOwnGrammar };
    let (mut pyramid, mut work, mut encoded) =
        (Pyramid::new(), Workspace::new(), crate::dsrn::Encoded::default());

    for (family, maps) in samples::every_family() {
        let (mut dsrn_bits, mut dsrn_nodes) = (0usize, 0usize);
        let (mut our_bits, mut our_nodes, mut our_subdivisions) = (0usize, 0usize, 0usize);

        for bitmap in &maps {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            encode(&pyramid, bitmap, knobs, &mut work, &mut encoded);
            dsrn_bits += encoded.bits();
            dsrn_nodes += encoded.counts.total_tiles();

            let (mut leaves, mut subdivisions) = (Vec::new(), 0usize);
            decide(&pyramid, bitmap, Region::whole_bitmap(), &mut leaves, &mut subdivisions);
            our_nodes += leaves.len();
            our_subdivisions += subdivisions;
            our_bits += leaves.iter().map(|leaf| payload_cost(leaf.region.level)).sum::<usize>();
        }

        let n = maps.len();
        println!(
            "\n  {family}, {n} bitmaps:\n    dsrn                {} bits, {} nodes a bitmap\n    tile-or-subdivide   {} bits, {} nodes, {} subdivisions a bitmap ({:+.1}% bits)",
            dsrn_bits / n,
            dsrn_nodes / n,
            our_bits / n,
            our_nodes / n,
            our_subdivisions / n,
            100.0 * (our_bits as f64 - dsrn_bits as f64) / dsrn_bits as f64
        );
    }
}
