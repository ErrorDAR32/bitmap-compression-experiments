//! What every cgt test checks of a bitmap, shared by the three tiers.

#![allow(dead_code)] // each test file uses only some of these

use bitmap::cgt::encoder::{read, write};
use bitmap::cgt::greedy_tiler::greedy_tiler;
use bitmap::cgt::pyramids::placements::Placements;
use bitmap::cgt::{decode, tree};
use bitmap::Bitmap;

/// The first cell, in reading order, where two bitmaps differ.
pub fn first_difference(a: &Bitmap, b: &Bitmap) -> Option<(u8, u8)> {
    (0..=u8::MAX).flat_map(|y| (0..=u8::MAX).map(move |x| (x, y))).find(|&(x, y)| a.get(x, y) != b.get(x, y))
}

/// Everything that has to hold of one bitmap, each failure naming what
/// broke:
///
/// - the greedy tiler's placed tiles cover every cell exactly once;
/// - the tree read back from the bits is the tree that was written;
/// - decoding gives back every cell.
pub fn check(bitmap: &Bitmap, label: &str) {
    let placements = greedy_tiler(bitmap);
    let covered: usize = placements.placed_tiles().map(|(tile, _)| tile.side_in_cells() * tile.side_in_cells()).sum();
    assert_eq!(covered, 256 * 256, "{label}: placed tiles leave cells uncovered or cover some twice");

    let written = tree(bitmap);
    let stream = write(&written, bitmap);
    assert!(read(&stream).tree == written, "{label}: the tree read back is not the tree written");

    let back = decode(&stream);
    if let Some((x, y)) = first_difference(bitmap, &back) {
        panic!("{label}: cell ({x}, {y}) comes back {} instead of {}", back.get(x, y), bitmap.get(x, y));
    }
}
