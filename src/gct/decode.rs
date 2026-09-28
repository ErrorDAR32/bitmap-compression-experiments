//! Decoding: read the stream back (the tree, and every cell it said
//! outright), then resolve the cells copies cover.
//!
//! A copy is chosen on content alone, so its source may not be resolved
//! yet when the tree reaches it -- it may even be a hole the residual
//! pass fills. So copies are resolved last, by repeated sweeps in
//! reading order, deferring a cell whenever its source is not known
//! yet. A copy always names something reading order puts before it, so
//! there is no cycle; an assertion backs that. Decoder speed is not a
//! goal here; simplicity is.

use crate::gct::encoder::bit_stream::BitStream;
use crate::gct::encoder::{read, ReadBack};
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{tile_side, Tile, CELL_LEVEL, DIRECTIONS};
use crate::gct::tree::node::{Node, Tree};
use crate::Bitmap;

/// Decodes a stream written by [`crate::gct::encode`].
pub fn decode(stream: &BitStream) -> Bitmap {
    let ReadBack { tree, mut cells, mut known } = read(stream);
    let mut left = (0..=u8::MAX).flat_map(|y| (0..=u8::MAX).map(move |x| (x, y))).filter(|&(x, y)| !known.get(x, y)).count();
    while left > 0 {
        let before = left;
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                if known.get(x, y) {
                    continue;
                }
                let (source_x, source_y) = copy_source(&tree, x, y);
                if !known.get(source_x, source_y) {
                    continue;
                }
                if cells.get(source_x, source_y) {
                    cells.set(x, y);
                }
                known.set(x, y);
                left -= 1;
            }
        }
        assert!(left < before, "nothing resolved in a whole sweep: a copy cycle, which should be impossible");
    }
    cells
}

/// The cell a copied cell reads from: the same cell of the copy's
/// source, one tile side away for a near copy, two for a far one (the
/// parent a far copy steps to is twice as wide).
fn copy_source(tree: &Pyramid, x: u8, y: u8) -> (u8, u8) {
    for level in 0..CELL_LEVEL {
        let side = tile_side(level);
        let tile = Tile { level, x: x as usize / side, y: y as usize / side };
        if let Node::Copied { far, direction } = tree.node(tile) {
            let step = (side * if far { 2 } else { 1 }) as isize;
            let (dx, dy) = DIRECTIONS[direction];
            return ((x as isize + dx * step) as u8, (y as isize + dy * step) as u8);
        }
    }
    unreachable!("a cell the stream did not say is always under a copy")
}
