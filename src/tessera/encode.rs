//! Encoding: the tree spelled out in bits, by the grammar
//! ([`crate::tessera::grammar`]) -- the start level, the tree node by node
//! from there, each complex tile's payload after its body -- then the
//! [last pass](crate::tessera::last_pass).

use crate::tessera::grammar::bit_stream::BitStream;
use crate::tessera::last_pass::LastPass;
use crate::tessera::set_counts::SetCounts;
use crate::tessera::grammar::cell_list;
use crate::tessera::grammar::*;
use crate::tessera::pyramids::tree::{Node, Tree};
use crate::tessera::tile::{cells_in_tile, Tile, CELL_LEVEL};
use crate::Bitmap;

/// Spells out `tree` for `bitmap` into `stream`, whatever it held
/// before, then the last pass, in `last_pass`: how many bits that took.
pub fn write(tree: &Tree, bitmap: &Bitmap, stream: &mut BitStream, last_pass: &mut LastPass) -> usize {
    stream.clear();
    stream.push_value(TREE_STREAM, STREAM_MODE_WIDTH);
    last_pass.clear();
    write_tree(tree, bitmap, stream, last_pass);
    let tree_end = stream.len();
    last_pass.encode(tree, bitmap, stream);
    stream.len() - tree_end
}

/// Spells out `bitmap`'s count split into `stream`, whatever it held
/// before; `set_counts` are `bitmap`'s.
pub fn write_count_split(bitmap: &Bitmap, set_counts: &SetCounts, stream: &mut BitStream) {
    stream.clear();
    stream.push_value(COUNT_SPLIT_STREAM, STREAM_MODE_WIDTH);
    count_split::write(bitmap, set_counts, stream);
}

/// Spells out `tree` for `bitmap` at the end of `stream`: the start
/// level, then every node from there, each copy noted in `last_pass`.
fn write_tree(tree: &Tree, bitmap: &Bitmap, stream: &mut BitStream, last_pass: &mut LastPass) {
    let start_level = tree.start_level();
    stream.push_value(start_level as u64, START_LEVEL_WIDTH);
    let mut writer = Writer { tree, bitmap, stream, last_pass };
    for tile in Tile::all_of_level(start_level) {
        writer.node(tile);
    }
}

/// What writing reads, and where it writes.
struct Writer<'a> {
    /// The tree spelled out.
    tree: &'a Tree,
    /// The bitmap whose values payloads say.
    bitmap: &'a Bitmap,
    /// Where the bits go.
    stream: &'a mut BitStream,
    /// Where copies are noted, for the last pass.
    last_pass: &'a mut LastPass,
}

impl Writer<'_> {
    /// Writes `tile`'s node and everything under it.
    fn node(&mut self, tile: Tile) {
        let node = self.tree.node(tile);
        match node {
            Node::Copied { far, direction, masks } => {
                self.stream.push_value(LEAF, LEAF_WIDTH);
                self.stream.push_value(COPY, CODE_WIDTH);
                self.stream.push_value(far as u64, FAR_WIDTH);
                self.stream.push_value(direction as u64, DIRECTION_WIDTH);
                if copy_or_divide_may_mask(tile.level) {
                    self.mask_present(masks);
                }
                if masks {
                    for child in tile.children() {
                        if self.tree.node(child) == Node::Absent {
                            self.last_pass.cover(tile, child, far, direction);
                        }
                    }
                    self.named_children(tile);
                } else {
                    self.last_pass.cover(tile, tile, far, direction);
                }
            }
            Node::MaskingBind => {
                // A divide that masks and flips the value bound above.
                self.stream.push_value(SUBDIVIDE, LEAF_WIDTH);
                self.stream.push_value(MASKING, MASK_PRESENT_WIDTH);
                self.stream.push_value(BINDING_FLIPPED, FLIP_WIDTH);
                self.named_children(tile);
            }
            Node::ComplexTile { size_offset } => {
                self.stream.push_value(LEAF, LEAF_WIDTH);
                self.stream.push_value(BIND, CODE_WIDTH);
                push_size_offset(self.stream, tile.level, size_offset);
                if has_payload_mode(tile.level, size_offset) {
                    self.stream.push_value(PLAIN_PAYLOAD, PAYLOAD_MODE_WIDTH);
                }
                write_payload(self.bitmap, tile, tile.level + size_offset, self.stream);
            }
            Node::Subdivided => {
                self.stream.push_value(SUBDIVIDE, LEAF_WIDTH);
                if self.tree.divides_whole(tile) {
                    if copy_or_divide_may_mask(tile.level) {
                        self.stream.push_value(NO_MASKING, MASK_PRESENT_WIDTH);
                    }
                    for child in tile.children() {
                        self.node(child);
                    }
                } else {
                    self.stream.push_value(MASKING, MASK_PRESENT_WIDTH);
                    self.stream.push_value(BINDING_KEPT, FLIP_WIDTH);
                    self.named_children(tile);
                }
            }
            Node::CellList => {
                let size_offset = CELL_LEVEL - tile.level;
                self.stream.push_value(LEAF, LEAF_WIDTH);
                self.stream.push_value(BIND, CODE_WIDTH);
                push_size_offset(self.stream, tile.level, size_offset);
                self.stream.push_value(CELL_LIST, PAYLOAD_MODE_WIDTH);
                cell_list::write(self.bitmap, tile, self.stream);
            }
            Node::Residual => self.stream.push_value(RESIDUAL, LEAF_WIDTH),
            Node::Absent => unreachable!("{node:?} is never written"),
        }
    }

    /// A mask-present bit: whether the node masks some of what it holds.
    fn mask_present(&mut self, masks: bool) {
        self.stream.push_value(if masks { MASKING } else { NO_MASKING }, MASK_PRESENT_WIDTH);
    }

    /// A masking node's child mask -- each child a node of its own, or said
    /// by the node: left to the binding above, or copied -- then the
    /// children that are nodes.
    fn named_children(&mut self, tile: Tile) {
        let children = tile.children();
        let named = children.map(|child| self.tree.node(child) != Node::Absent);
        for is_named in named {
            self.stream.push_value(if is_named { MASKED } else { UNMASKED }, MASK_BIT_WIDTH);
        }
        for (child, is_named) in children.into_iter().zip(named) {
            if is_named {
                self.node(child);
            }
        }
    }
}

/// A complex tile's payload: the value of each of the complex tile
/// `tile`'s tiles of `resolution`, in Morton order. At 1x1, those are
/// its cells, as they lie in the bitmap: written a word at a time.
fn write_payload(bitmap: &Bitmap, tile: Tile, resolution: u8, stream: &mut BitStream) {
    if resolution == CELL_LEVEL {
        let cells = cells_in_tile(tile.level) as usize;
        for word in bitmap.square_words(tile.top_left_cell(), tile.side_in_cells()) {
            stream.push_value(word, cells.min(u64::BITS as usize) as u8);
        }
        return;
    }
    for resolution_tile in tile.tiles_at_size_offset(resolution - tile.level) {
        stream.push(resolution_tile.top_left_value(bitmap));
    }
}
