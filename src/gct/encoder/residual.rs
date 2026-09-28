//! The residual pass: one raw bit for every cell of every residual 2x2
//! the tree leaves, in reading order, after the whole tree -- the cells
//! no bound or copied tile of 2x2 or coarser covers.

use super::bit_stream::{BitReader, BitStream};
use super::StreamContents;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{Tile, CELL_LEVEL};
use crate::gct::pyramids::tree::{Node, Tree};
use crate::Bitmap;

/// Every cell of every residual 2x2.
fn residual_cell_bitmap(tree: &Pyramid) -> Bitmap {
    let mut residual = Bitmap::new();
    for tile in Tile::all_of_level(CELL_LEVEL - 1) {
        if tree.node(tile) == Node::Residual {
            tile.set_in(&mut residual);
        }
    }
    residual
}

/// The residual cells, in reading order.
fn residual_cells(tree: &Pyramid) -> impl Iterator<Item = Tile> {
    let residual = residual_cell_bitmap(tree);
    Tile::all_cells().filter(move |cell| cell.top_left_value(&residual))
}

pub fn write_residual(tree: &Pyramid, bitmap: &Bitmap, out: &mut BitStream) {
    for cell in residual_cells(tree) {
        out.push(cell.top_left_value(bitmap));
    }
}

pub fn read_residual(reader: &mut BitReader, read: &mut StreamContents) {
    for cell in residual_cells(&read.tree).collect::<Vec<_>>() {
        read.bind(cell, reader.bit());
    }
}
