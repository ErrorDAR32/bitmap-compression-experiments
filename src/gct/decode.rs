//! Decoding: read the stream back by the grammar
//! ([`crate::gct::grammar`]) -- the tree, and every cell the tree binds
//! outright -- then the [last pass](crate::gct::last_pass): the blocks
//! copies cover, copied, and the residual blocks' cells.

use crate::gct::grammar::bit_stream::{BitReader, BitStream};
use crate::gct::grammar::order::payload_parts;
use crate::gct::grammar::cell_list;
use crate::gct::grammar::*;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::last_pass::LastPass;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{Tile, CELL_LEVEL, FLOOR_LEVEL};
use crate::Bitmap;

/// Where reading a stream back writes: the tree, and every cell.
pub struct StreamContents<'a> {
    /// The tree the stream spells out.
    pub tree: &'a mut Tree,
    /// Every cell's value.
    pub cell_values: &'a mut Bitmap,
    /// The last pass: the blocks copies cover, and where each copies
    /// from -- the offsets the stream was encoded with.
    pub last_pass: &'a mut LastPass,
}

/// Reads back what [`crate::gct::encode::write`] wrote, into `read`,
/// whatever it held before: the tree and every cell, or, for a stream
/// that is a count split, no tree and every cell.
pub fn decode(stream: &BitStream, read: &mut StreamContents) {
    read.tree.clear();
    read.cell_values.reset();
    read.last_pass.clear();
    let mut reader = stream.reader();
    if reader.value(STREAM_MODE_WIDTH) == COUNT_SPLIT_STREAM {
        count_split::read(&mut reader, read.cell_values);
        return;
    }
    let start_level = reader.value(START_LEVEL_WIDTH) as u8;
    for level in 0..start_level {
        for tile in Tile::all_of_level(level) {
            read.tree.set_node(tile, Node::Subdivided);
        }
    }
    for tile in Tile::all_of_level(start_level) {
        read.node(&mut reader, tile, &mut NestedResolutions::none(), BOUND_AT_THE_TOP);
    }
    read.last_pass.decode(read.tree, read.cell_values, &mut reader);
}

impl StreamContents<'_> {
    /// Reads `tile`'s node and everything under it, `bound_above` the
    /// value bound above it.
    fn node(&mut self, reader: &mut BitReader, tile: Tile, nested: &mut NestedResolutions, bound_above: bool) {
        for nesting in nested.able_to_unmask(tile) {
            if reader.value(MASK_BIT_WIDTH) == UNMASKED {
                self.tree.set_node(tile, Node::Unmasked { nesting });
                return;
            }
        }

        let leaf = reader.value(LEAF_WIDTH) == LEAF;
        if !leaf && tile.level == FLOOR_LEVEL {
            self.tree.set_node(tile, Node::Residual);
            return;
        }
        if !leaf {
            if !copy_or_divide_may_mask(tile.level) || reader.value(MASK_PRESENT_WIDTH) == NO_MASKING {
                self.tree.set_node(tile, Node::Subdivided);
                for child in tile.children() {
                    self.node(reader, child, nested, bound_above);
                }
                return;
            }
            let flips = reader.value(FLIP_WIDTH) == BINDING_FLIPPED;
            let bound_inside = bound_above != flips;
            self.tree.set_node(tile, if flips { Node::ComplexTile { size_offset: 0, masks: true } } else { Node::Subdivided });
            self.named_children(reader, tile, nested, bound_inside, |read, child| {
                if bound_inside {
                    child.set_in(read.cell_values);
                }
            });
            return;
        }
        if reader.value(CODE_WIDTH) == COPY {
            let far = reader.value(FAR_WIDTH) != 0;
            let direction = reader.value(DIRECTION_WIDTH) as u8;
            let masks = copy_or_divide_may_mask(tile.level) && reader.value(MASK_PRESENT_WIDTH) == MASKING;
            self.tree.set_node(tile, Node::Copied { far, direction, masks });
            if masks {
                self.named_children(reader, tile, nested, bound_above, |read, child| read.last_pass.cover(tile, child, far, direction));
            } else {
                self.last_pass.cover(tile, tile, far, direction);
            }
            return;
        }
        let size_offset = reader.value(bind_resolution_width(tile.level, nested.in_body())) as u8;
        let masks = complex_tile_may_mask(size_offset) && reader.value(MASK_PRESENT_WIDTH) == MASKING;
        if has_payload_mode(tile.level, size_offset, masks) && reader.value(PAYLOAD_MODE_WIDTH) == CELL_LIST {
            self.tree.set_node(tile, Node::CellList);
            cell_list::read(reader, tile, self.cell_values);
            return;
        }
        self.tree.set_node(tile, Node::ComplexTile { size_offset, masks });
        let nesting = nested.next_nesting();
        if masks {
            nested.while_nested(tile.level + size_offset, |inside| {
                for child in tile.children() {
                    self.node(reader, child, inside, bound_above);
                }
            });
        }
        self.payload(reader, tile, nesting, size_offset);
    }

    /// A masking node's child mask, then the children that are nodes, each
    /// read with `bound_above` the value bound above it; every other child
    /// is said by the node, as `say_unnamed` does.
    fn named_children(
        &mut self,
        reader: &mut BitReader,
        tile: Tile,
        nested: &mut NestedResolutions,
        bound_above: bool,
        mut say_unnamed: impl FnMut(&mut Self, Tile),
    ) {
        let children = tile.children();
        let named = children.map(|_| reader.value(MASK_BIT_WIDTH) == MASKED);
        for (child, is_named) in children.into_iter().zip(named) {
            if is_named {
                self.node(reader, child, nested, bound_above);
            } else {
                say_unnamed(self, child);
            }
        }
    }

    /// A complex tile's payload, bound into the cells of the tiles it
    /// names, a part at a time.
    fn payload(&mut self, reader: &mut BitReader, tile: Tile, nesting: u8, size_offset: u8) {
        let resolution = tile.level + size_offset;
        for part in payload_parts(self.tree, tile, nesting) {
            read_part(reader, part, resolution, self.cell_values);
        }
    }
}

/// One part of a payload, as [`crate::gct::encode`](mod@crate::gct::encode) writes it: at 1x1,
/// its cells a word at a time; otherwise each tile of `resolution` bound
/// to its value.
fn read_part(reader: &mut BitReader, part: Tile, resolution: u8, cells: &mut Bitmap) {
    if resolution == CELL_LEVEL {
        let (corner, side) = (part.top_left_cell(), part.side_in_cells());
        let count = side * side;
        if count < u64::BITS as usize {
            cells.set_in_small_square(corner, side, reader.value(count as u8));
        } else {
            for word in cells.square_words_mut(corner, side) {
                *word |= reader.value(u64::BITS as u8);
            }
        }
        return;
    }
    for tile in part.tiles_at_size_offset(resolution - part.level) {
        if reader.bit() {
            tile.set_in(cells);
        }
    }
}
