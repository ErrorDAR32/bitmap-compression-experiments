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
                let (left, top, right, bottom) = tile.cell_rect();
                holes.set_rect(left as i64, top as i64, right as i64, bottom as i64);
            }
        }
    }
    holes
}

/// The hole cells, in reading order.
fn residual_cells(tree: &Pyramid) -> impl Iterator<Item = (u8, u8)> {
    let holes = hole_cells(tree);
    (0..=u8::MAX).flat_map(move |y| (0..=u8::MAX).map(move |x| (x, y))).filter(move |&(x, y)| holes.get(x, y))
}

pub fn write_residual(tree: &Pyramid, bitmap: &Bitmap, out: &mut BitStream) {
    for (x, y) in residual_cells(tree) {
        out.push(bitmap.get(x, y));
    }
}

pub fn read_residual(reader: &mut BitReader, read: &mut ReadBack) {
    for (x, y) in residual_cells(&read.tree).collect::<Vec<_>>() {
        read.fill(Tile { level: CELL_LEVEL, x: x as usize, y: y as usize }, reader.bit());
    }
}
