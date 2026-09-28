//! The residual pass: one raw bit for every cell of every hole the tree
//! leaves, in reading order, after the whole tree -- the cells no tile
//! of 2x2 or coarser says.

use super::bit_stream::{BitReader, BitStream};
use super::ReadBack;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{tiles_across, Tile, CELL_LEVEL};
use crate::gct::pyramids::tree::{Node, Tree};
use crate::Bitmap;

/// Every cell of every hole.
fn hole_cells(tree: &Pyramid) -> Bitmap {
    let mut holes = Bitmap::new();
    let level = CELL_LEVEL - 1;
    let across = tiles_across(level);
    for y in 0..across {
        for x in 0..across {
            let tile = Tile { level, x, y };
            if tree.node(tile) == Node::Hole {
                tile.set_in(&mut holes);
            }
        }
    }
    holes
}

/// The hole cells, in reading order.
fn residual_cells(tree: &Pyramid) -> impl Iterator<Item = Tile> {
    let holes = hole_cells(tree);
    Tile::all_cells().filter(move |cell| cell.top_left_value(&holes))
}

pub fn write_residual(tree: &Pyramid, bitmap: &Bitmap, out: &mut BitStream) {
    for cell in residual_cells(tree) {
        out.push(cell.top_left_value(bitmap));
    }
}

pub fn read_residual(reader: &mut BitReader, read: &mut ReadBack) {
    for cell in residual_cells(&read.tree).collect::<Vec<_>>() {
        read.fill(cell, reader.bit());
    }
}
