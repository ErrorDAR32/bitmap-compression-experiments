//! What every gct test checks of a bitmap, shared by the three tiers.

#![allow(dead_code)] // each test file uses only some of these

pub mod tree_stats;

use bitmap::gct::decode::read;
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

/// The first cell, in reading order, where two bitmaps differ.
pub fn first_difference(a: &Bitmap, b: &Bitmap) -> Option<(u8, u8)> {
    (0..=u8::MAX).flat_map(|y| (0..=u8::MAX).map(move |x| (x, y))).find(|&(x, y)| a.get(x, y) != b.get(x, y))
}

/// Everything that has to hold of one bitmap, each failure naming what
/// broke:
///
/// - the greedy tiler's placed tiles cover every cell exactly once;
/// - nothing finer than 4x4 is copied;
/// - every 1x1 tile placed is left to the residual pass, under a
///   residual 2x2 -- never said by the tree;
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
    for (tile, placement) in placements.placed_tiles() {
        if let Placement::Copied { .. } = placement {
            assert!(tile.level < CELL_LEVEL - 1, "{label}: {tile:?} copies, finer than 4x4");
        }
        if tile.level == CELL_LEVEL {
            let floor = tile.ancestor(CELL_LEVEL - 1);
            assert_eq!(written.node(floor), Node::Residual, "{label}: 1x1 {tile:?} is not left to the residual pass");
        }
    }

    let stream = write(&written, bitmap);
    assert!(read(&stream).tree == written, "{label}: the tree read back is not the tree written");

    let back = decode(&stream);
    if let Some((x, y)) = first_difference(bitmap, &back) {
        panic!("{label}: cell ({x}, {y}) comes back {} instead of {}", back.get(x, y), bitmap.get(x, y));
    }
}
