//! What has to be true of the greedy pass, checked against the same
//! corpora the rest of dsrn_exp measures on.

#![cfg(test)]

use super::greedy_tiles::greedy_tile_pass;
use crate::pyramid::{tile_side, Pyramid, CELL_LEVEL};
use crate::samples;

/// Every tile the pass places covers area that adds up to exactly the
/// whole bitmap, once -- no gaps, and the "claimed" check already
/// rules out overlaps.
#[test]
fn covers_every_cell_exactly_once() {
    let mut pyramid = Pyramid::new();
    for (_, maps) in samples::every_family() {
        for bitmap in &maps {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            let counts = greedy_tile_pass(&pyramid, bitmap);
            let area: usize = (0..=CELL_LEVEL)
                .map(|level| {
                    let side = tile_side(level);
                    (counts.bound_at_level[level] + counts.copied_at_level[level]) * side * side
                })
                .sum();
            assert_eq!(area, 256 * 256, "left cells uncovered or double-covered");
        }
    }
}
