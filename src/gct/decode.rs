//! Decoding: read the stream back by the grammar
//! ([`crate::gct::grammar`]) -- the tree, and every cell the stream binds
//! outright -- then resolve the cells copies cover.
//!
//! A copy is chosen on content alone, so its source may not be resolved
//! yet when the tree reaches it -- it may even be a residual cell the
//! residual pass binds. So copies are resolved last, by repeated sweeps in
//! reading order, deferring a cell whenever its source is not known
//! yet. A copy always names something reading order puts before it, so
//! there is no cycle; an assertion backs that. Decoder speed is not a
//! goal here; simplicity is.

use crate::gct::grammar::bit_stream::{BitReader, BitStream};
use crate::gct::grammar::order::{payload_tiles, residual_cells};
use crate::gct::grammar::*;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::copyable::{FAR_DISTANCE, NEAR_DISTANCE};
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{tile_side, Tile, CELL_LEVEL};
use crate::Bitmap;

/// What reading a stream back gives: the tree, and every cell whose
/// value the stream binds outright -- all but the cells copies cover.
pub struct StreamContents {
    pub tree: Pyramid,
    pub cell_values: Bitmap,
    pub known_cells: Bitmap,
}

impl StreamContents {
    /// Binds every cell of `tile` to `value`.
    fn bind(&mut self, tile: Tile, value: bool) {
        if value {
            tile.set_in(&mut self.cell_values);
        }
        tile.set_in(&mut self.known_cells);
    }
}

/// Reads back what [`crate::gct::encode::write`] wrote.
pub fn read(stream: &BitStream) -> StreamContents {
    let mut read = StreamContents { tree: Pyramid::tree(), cell_values: Bitmap::new(), known_cells: Bitmap::new() };
    let mut reader = stream.reader();
    let start_level = reader.value(START_LEVEL_WIDTH) as u8;
    for level in 0..start_level {
        for tile in Tile::all_of_level(level) {
            read.tree.set_node(tile, Node::Subdivided);
        }
    }
    for tile in Tile::all_of_level(start_level) {
        read_node(&mut reader, tile, &mut NestedResolutions::none(), &mut read);
    }
    for cell in residual_cells(&read.tree).collect::<Vec<_>>() {
        read.bind(cell, reader.bit());
    }
    read
}

/// Reads `tile`'s node and everything under it.
fn read_node(reader: &mut BitReader, tile: Tile, nested: &mut NestedResolutions, read: &mut StreamContents) {
    let able_to_unmask: Vec<u8> = nested.able_to_unmask(tile).collect();
    for nesting in able_to_unmask {
        if reader.value(MASK_BIT_WIDTH) == UNMASKED {
            read.tree.set_node(tile, Node::Unmasked { nesting });
            return;
        }
    }

    let leaf = reader.value(LEAF_WIDTH) == LEAF;
    if tile.level == CELL_LEVEL - 1 {
        if !leaf {
            read.tree.set_node(tile, Node::Residual);
            return;
        }
        read.tree.set_node(tile, Node::ComplexTile { size_offset: 0, masks: false });
        read_payload(reader, tile, nested.next_nesting(), 0, read);
        return;
    }
    if !leaf {
        read.tree.set_node(tile, Node::Subdivided);
        for child in tile.children() {
            read_node(reader, child, nested, read);
        }
        return;
    }
    if reader.value(CODE_WIDTH) == COPY {
        let far = reader.value(FAR_WIDTH) != 0;
        let direction = reader.value(DIRECTION_WIDTH) as u8;
        let masks = copy_may_mask(tile.level) && reader.value(MASK_PRESENT_WIDTH) == MASKING;
        read.tree.set_node(tile, Node::Copied { far, direction, masks });
        if masks {
            let mut masked = Vec::new();
            for child in tile.children() {
                if reader.value(MASK_BIT_WIDTH) == MASKED {
                    masked.push(child);
                }
            }
            for child in masked {
                read_node(reader, child, nested, read);
            }
        }
        return;
    }
    let size_offset = reader.value(resolution_width(tile.level)) as u8;
    let masks = size_offset > 1 && reader.value(MASK_PRESENT_WIDTH) == MASKING;
    read.tree.set_node(tile, Node::ComplexTile { size_offset, masks });
    let nesting = nested.next_nesting();
    if masks {
        nested.while_nested(tile.level + size_offset, |inside| {
            for child in tile.children() {
                read_node(reader, child, inside, read);
            }
        });
    }
    read_payload(reader, tile, nesting, size_offset, read);
}

/// A complex tile's payload, bound into the cells of the tiles it names.
fn read_payload(reader: &mut BitReader, tile: Tile, nesting: u8, size_offset: u8, read: &mut StreamContents) {
    for part in payload_tiles(&read.tree, tile, nesting, size_offset) {
        read.bind(part, reader.bit());
    }
}

/// Decodes a stream written by [`crate::gct::encode`](fn@crate::gct::encode).
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

/// The cell a copied cell reads from: the same cell of its nearest
/// copy's source, one tile side away for a near copy, two for a far
/// one. The nearest, since a masking copy's masked children may copy
/// again, from somewhere else.
fn copy_source(tree: &Pyramid, cell: Tile) -> Tile {
    for level in (0..CELL_LEVEL).rev() {
        if let Node::Copied { far, direction, .. } = tree.node(cell.ancestor(level)) {
            let distance = if far { FAR_DISTANCE } else { NEAR_DISTANCE };
            return cell
                .neighbour_at(direction, tile_side(level) * distance)
                .expect("a copy always reads from inside the bitmap");
        }
    }
    unreachable!("a cell the stream did not say is always under a copy")
}
