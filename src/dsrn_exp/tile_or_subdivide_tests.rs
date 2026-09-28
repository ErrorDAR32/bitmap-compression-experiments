//! Whether the tile-or-subdivide codec round-trips: every copy's
//! source has to actually be resolved by the time a single-pass
//! decoder reads it, not just eligible by content.

#![cfg(test)]

use super::greedy_tiles::{complex_tiler, greedy_tiler};
use super::tile_or_subdivide::{decode, encode, parse_whole_tree, tree_reproduces, write_whole_tree};
use crate::pyramid::Pyramid;
use crate::samples;

#[test]
fn round_trips_every_sample() {
    let mut pyramid = Pyramid::new();
    for (family, maps) in samples::every_family() {
        for (case, bitmap) in maps.iter().enumerate() {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            let back = decode(&encode(&pyramid, bitmap));
            for y in 0..=u8::MAX {
                for x in 0..=u8::MAX {
                    assert_eq!(
                        bitmap.get(x, y),
                        back.get(x, y),
                        "{family}, case {case}: differs at ({x}, {y})"
                    );
                }
            }
        }
    }
}

/// Checks `complex_tiler`'s own tree-building, independent of the
/// bitstream grammar entirely: walking the tree it built and filling
/// whatever it leaves uncovered straight from the bitmap reproduces the
/// bitmap exactly.
#[test]
fn tree_reproduces_every_sample() {
    let mut pyramid = Pyramid::new();
    for (family, maps) in samples::every_family() {
        for (case, bitmap) in maps.iter().enumerate() {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            let tree = complex_tiler(&greedy_tiler(&pyramid, bitmap));
            assert!(tree_reproduces(&tree, bitmap), "{family}, case {case}: tree does not reproduce the bitmap");
        }
    }
}

/// Checks the bitstream grammar independent of the tree builder: parsing
/// what was just written reconstructs the exact same tree.
#[test]
fn parses_back_every_written_tree() {
    let mut pyramid = Pyramid::new();
    for (family, maps) in samples::every_family() {
        for (case, bitmap) in maps.iter().enumerate() {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            let tree = complex_tiler(&greedy_tiler(&pyramid, bitmap));
            let stream = write_whole_tree(&tree);
            let parsed = parse_whole_tree(&stream);
            assert_eq!(tree, parsed, "{family}, case {case}: parsed tree differs from what was written");
        }
    }
}
