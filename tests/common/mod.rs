//! What every gct test checks of a bitmap, shared by the three tiers.

#![allow(dead_code)] // each test file uses only some of these

pub mod tree_stats;

use bitmap::gct::complex_tiler::bit_cost::bits;
use bitmap::gct::complex_tiler::complex_tiler::complex_tiler;
use bitmap::gct::decode::read;
use bitmap::gct::grammar::{BOUND_AT_THE_TOP, START_LEVEL_WIDTH};
use bitmap::gct::nested_resolutions::NestedResolutions;
use bitmap::gct::encode::write;
use bitmap::gct::greedy_tiler::greedy_tiler;
use bitmap::gct::pyramids::copyable::Copyable;
use bitmap::gct::pyramids::homogeneity::Homogeneity;
use bitmap::gct::pyramids::placements::{Placement, Placements};
use bitmap::gct::pyramids::pyramid::Pyramid;
use bitmap::gct::pyramids::tree::{Node, Tree};
use bitmap::gct::tile::{Tile, CELL_LEVEL};
use bitmap::gct::{decode, tree};
use bitmap::Bitmap;

/// The raw cells: what a bitmap costs written out.
const RAW_CELLS: usize = 256 * 256;

/// The most gct may ever spend on a bitmap: the raw cells and 1%.
pub const CAP_BITS: usize = RAW_CELLS + RAW_CELLS / 100;

/// The first cell, in reading order, where two bitmaps differ.
pub fn first_difference(a: &Bitmap, b: &Bitmap) -> Option<(u8, u8)> {
    (0..=u8::MAX).flat_map(|y| (0..=u8::MAX).map(move |x| (x, y))).find(|&(x, y)| a.get(x, y) != b.get(x, y))
}

/// Everything that has to hold of one bitmap, each failure naming what
/// broke:
///
/// - the greedy tiler's placed tiles cover every cell exactly once;
/// - nothing finer than 4x4 is copied;
/// - every 1x1 tile placed is said raw: left to the residual pass under
///   a residual 2x2, or inside a complex tile of 1x1 resolution;
/// - the bit cost the complex tiler scores with is the encoder's count;
/// - gct spends at most [`CAP_BITS`], the raw cells and 1%;
/// - the tree read back from the bits is the tree that was written;
/// - decoding gives back every cell.
pub fn check(bitmap: &Bitmap, label: &str) {
    let placements = greedy_tiler(bitmap, &Pyramid::homogeneity(bitmap), &Pyramid::copyable(bitmap));
    let cells = |tile: Tile| tile.side_in_cells() * tile.side_in_cells();
    let covered: usize = placements
        .placed_tiles()
        .map(|(tile, placement)| {
            let masked = if placement.masks_any() {
                tile.children().into_iter().filter(|&child| placement.masks(child)).map(cells).sum()
            } else {
                0
            };
            cells(tile) - masked
        })
        .sum();
    assert_eq!(covered, 256 * 256, "{label}: placed tiles leave cells uncovered or cover some twice");

    let written = tree(bitmap);
    let stream = write(&written, bitmap);
    let complex_tiling = complex_tiler(&placements);
    let start_level = written.start_level();
    let counted: u64 =
        Tile::all_of_level(start_level).map(|tile| bits(&complex_tiling, tile, &mut NestedResolutions::none(), BOUND_AT_THE_TOP)).sum();
    assert_eq!(
        counted + START_LEVEL_WIDTH as u64,
        stream.len() as u64,
        "{label}: the complex tiler's bit cost is not the encoder's count"
    );
    assert!(stream.len() <= CAP_BITS, "{label}: {} bits, over the cap of {CAP_BITS}", stream.len());
    for (tile, placement) in placements.placed_tiles() {
        if let Placement::Copied { .. } = placement {
            assert!(tile.level < CELL_LEVEL - 1, "{label}: {tile:?} copies, finer than 4x4");
        }
        if tile.level == CELL_LEVEL {
            let residual = written.node(tile.ancestor(CELL_LEVEL - 1)) == Node::Residual;
            let raw = (0..CELL_LEVEL).any(|level| {
                matches!(written.node(tile.ancestor(level)), Node::ComplexTile { size_offset, .. } if level + size_offset == CELL_LEVEL)
            });
            assert!(residual || raw, "{label}: 1x1 {tile:?} is neither residual nor in a raw complex tile");
        }
    }

    assert!(read(&stream).tree == written, "{label}: the tree read back is not the tree written");

    let back = decode(&stream);
    if let Some((x, y)) = first_difference(bitmap, &back) {
        panic!("{label}: cell ({x}, {y}) comes back {} instead of {}", back.get(x, y), bitmap.get(x, y));
    }
}
