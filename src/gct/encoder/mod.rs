//! The one place the grammar lives: turning the complex tiler's tree
//! into bits, and bits back into a tree. The tree's own grammar
//! (`tree_grammar`), with each complex tile's payload (`payload`) after
//! its body, then the residual pass (`residual`) -- the last two being
//! plain runs of value bits. Nothing here decides anything; resolving
//! copies into cells is [`crate::gct::decode`](mod@crate::gct::decode)'s job.

pub mod bit_stream;
mod payload;
mod residual;
mod tree_grammar;

use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::Tile;
use crate::gct::enclosing::Enclosing;
use crate::gct::tree::node::Tree;
use crate::Bitmap;
use bit_stream::BitStream;

/// Spells out `tree` for `bitmap`: the tree with its payloads, then
/// the residual pass.
pub fn write(tree: &Pyramid, bitmap: &Bitmap) -> BitStream {
    let mut out = BitStream::default();
    tree_grammar::write_node(tree, bitmap, Tile::whole_bitmap(), &mut Enclosing::none(), &mut out);
    residual::write_residual(tree, bitmap, &mut out);
    out
}

/// What reading a stream back gives: the tree, and every cell whose
/// value the stream said outright -- all but the cells copies cover.
pub struct ReadBack {
    pub tree: Pyramid,
    pub cells: Bitmap,
    pub known: Bitmap,
}

impl ReadBack {
    /// Every cell of `tile` holds `value`.
    fn fill(&mut self, tile: Tile, value: bool) {
        let (left, top, right, bottom) = tile.cell_rect();
        if value {
            self.cells.set_rect(left as i64, top as i64, right as i64, bottom as i64);
        }
        self.known.set_rect(left as i64, top as i64, right as i64, bottom as i64);
    }
}

/// Reads back what [`write`](fn@write) wrote.
pub fn read(stream: &BitStream) -> ReadBack {
    let mut read = ReadBack { tree: Pyramid::tree(), cells: Bitmap::new(), known: Bitmap::new() };
    let mut reader = stream.reader();
    tree_grammar::read_node(&mut reader, Tile::whole_bitmap(), &mut Enclosing::none(), &mut read);
    residual::read_residual(&mut reader, &mut read);
    read
}
