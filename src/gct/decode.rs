//! Decoding: read the stream back (the tree, and every cell it said
//! outright), then resolve the cells copies cover.
//!
//! A copy is chosen on content alone, so its source may not be resolved
//! yet when the tree reaches it -- it may even be a residual cell the
//! residual pass binds. So copies are resolved last, by repeated sweeps in
//! reading order, deferring a cell whenever its source is not known
//! yet. A copy always names something reading order puts before it, so
//! there is no cycle; an assertion backs that. Decoder speed is not a
//! goal here; simplicity is.

use crate::gct::encoder::bit_stream::BitStream;
use crate::gct::encoder::{read, StreamContents};
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::copyable::{FAR_DISTANCE, NEAR_DISTANCE};
use crate::gct::tile::{tile_side, Tile, CELL_LEVEL};
use crate::gct::pyramids::tree::{Node, Tree};
use crate::Bitmap;

/// Decodes a stream written by [`crate::gct::encode`].
pub fn decode(stream: &BitStream) -> Bitmap {
    let StreamContents { tree, mut cell_values, mut known_cells } = read(stream);
    let mut left = Tile::all_cells().filter(|&cell| !cell.top_left_value(&known_cells)).count();
    while left > 0 {
        let before = left;
        for cell in Tile::all_cells() {
            if cell.top_left_value(&known_cells) {
                continue;
            }
            let source = copy_source(&tree, cell);
            if !source.top_left_value(&known_cells) {
                continue;
            }
            if source.top_left_value(&cell_values) {
                cell.set_in(&mut cell_values);
            }
            cell.set_in(&mut known_cells);
            left -= 1;
        }
        assert!(left < before, "nothing resolved in a whole sweep: a copy cycle, which should be impossible");
    }
    cell_values
}

/// The cell a copied cell reads from: the same cell of the copy's
/// source, one tile side away for a near copy, two for a far one.
fn copy_source(tree: &Pyramid, cell: Tile) -> Tile {
    for level in 0..CELL_LEVEL {
        if let Node::Copied { far, direction } = tree.node(cell.ancestor(level)) {
            let distance = if far { FAR_DISTANCE } else { NEAR_DISTANCE };
            return cell
                .neighbour_at(direction, tile_side(level) * distance)
                .expect("a copy always reads from inside the bitmap");
        }
    }
    unreachable!("a cell the stream did not say is always under a copy")
}
