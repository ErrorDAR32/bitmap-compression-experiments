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
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::tree::Tree;
use crate::Bitmap;
use bit_stream::BitStream;

/// Spells out `tree` for `bitmap`: the tree with its payloads, then
/// the residual pass.
pub fn write(tree: &Pyramid, bitmap: &Bitmap) -> BitStream {
    let mut out = BitStream::default();
    tree_grammar::write_node(tree, bitmap, Tile::whole_bitmap(), &mut NestedResolutions::none(), &mut out);
    residual::write_residual(tree, bitmap, &mut out);
    out
}

/// What reading a stream back gives: the tree, and every cell whose
/// value the stream said outright -- all but the cells copies cover.
pub struct StreamContents {
    pub tree: Pyramid,
    pub cell_values: Bitmap,
    pub known_cells: Bitmap,
}

impl StreamContents {
    /// Every cell of `tile` holds `value`.
    fn bind(&mut self, tile: Tile, value: bool) {
        if value {
            tile.set_in(&mut self.cell_values);
        }
        tile.set_in(&mut self.known_cells);
    }
}

/// Reads back what [`write`](fn@write) wrote.
pub fn read(stream: &BitStream) -> StreamContents {
    let mut read = StreamContents { tree: Pyramid::tree(), cell_values: Bitmap::new(), known_cells: Bitmap::new() };
    let mut reader = stream.reader();
    tree_grammar::read_node(&mut reader, Tile::whole_bitmap(), &mut NestedResolutions::none(), &mut read);
    residual::read_residual(&mut reader, &mut read);
    read
}
