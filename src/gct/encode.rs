//! Encoding: the tree spelled out in bits, by the grammar
//! ([`crate::gct::grammar`]) -- the start level, the tree node by node
//! from there, each complex tile's payload after its body, then the
//! residual pass.

use crate::gct::grammar::bit_stream::BitStream;
use crate::gct::grammar::order::PayloadWalk;
use crate::gct::grammar::point_list;
use crate::gct::grammar::*;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{cells_in_tile, Tile, CELL_LEVEL};
use crate::Bitmap;

/// Spells out `tree` for `bitmap` into `stream`, whatever it held before;
/// `payload_walk` is room for a payload's parts.
pub fn write(tree: &Pyramid, bitmap: &Bitmap, stream: &mut BitStream, payload_walk: &mut PayloadWalk) {
    stream.clear();
    let start_level = tree.start_level();
    stream.push_value(start_level as u64, START_LEVEL_WIDTH);
    let mut writer = Writer { tree, bitmap, stream, payload_walk };
    for tile in Tile::all_of_level(start_level) {
        writer.node(tile, &mut NestedResolutions::none());
    }
    for square in tree.residual_squares() {
        writer.stream.push_value(bitmap.small_square(square.top_left_cell(), square.side_in_cells()), RESIDUAL_SQUARE_BITS);
    }
}

/// What writing reads, and where it writes.
struct Writer<'a> {
    /// The tree spelled out.
    tree: &'a Pyramid,
    /// The bitmap whose values payloads say.
    bitmap: &'a Bitmap,
    /// Where the bits go.
    stream: &'a mut BitStream,
    /// Room for a payload's parts.
    payload_walk: &'a mut PayloadWalk,
}

impl Writer<'_> {
    /// Writes `tile`'s node and everything under it.
    fn node(&mut self, tile: Tile, nested: &mut NestedResolutions) {
        let node = self.tree.node(tile);
        for nesting in nested.able_to_unmask(tile) {
            if node == (Node::Unmasked { nesting }) {
                self.stream.push_value(UNMASKED, MASK_BIT_WIDTH);
                return;
            }
            self.stream.push_value(MASKED, MASK_BIT_WIDTH);
        }

        if tile.level == CELL_LEVEL - 1 {
            match node {
                Node::ComplexTile { size_offset: 0, .. } => {
                    self.stream.push_value(LEAF, LEAF_WIDTH);
                    self.payload(tile, nested.next_nesting(), 0);
                }
                Node::Residual => self.stream.push_value(RESIDUAL, LEAF_WIDTH),
                _ => unreachable!("the 2x2 floor is always a bound tile or residual"),
            }
            return;
        }

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
                    self.named_children(tile, nested);
                }
            }
            Node::ComplexTile { size_offset: 0, masks: true } => {
                // A bind that masks: a divide that masks and flips the value
                // bound above.
                self.stream.push_value(SUBDIVIDE, LEAF_WIDTH);
                self.stream.push_value(MASKING, MASK_PRESENT_WIDTH);
                self.stream.push_value(BINDING_FLIPPED, FLIP_WIDTH);
                self.named_children(tile, nested);
            }
            Node::ComplexTile { size_offset, masks } => {
                self.stream.push_value(LEAF, LEAF_WIDTH);
                self.stream.push_value(BIND, CODE_WIDTH);
                self.stream.push_value(size_offset as u64, resolution_width(tile.level));
                if complex_tile_may_mask(size_offset) {
                    self.mask_present(masks);
                }
                if has_payload_mode(tile.level, size_offset, masks) {
                    self.stream.push_value(PLAIN_PAYLOAD, PAYLOAD_MODE_WIDTH);
                }
                let nesting = nested.next_nesting();
                if masks {
                    nested.while_nested(tile.level + size_offset, |inside| {
                        for child in tile.children() {
                            self.node(child, inside);
                        }
                    });
                }
                self.payload(tile, nesting, size_offset);
            }
            Node::Subdivided => {
                self.stream.push_value(SUBDIVIDE, LEAF_WIDTH);
                if self.tree.divides_whole(tile) {
                    if copy_or_divide_may_mask(tile.level) {
                        self.stream.push_value(NO_MASKING, MASK_PRESENT_WIDTH);
                    }
                    for child in tile.children() {
                        self.node(child, nested);
                    }
                } else {
                    self.stream.push_value(MASKING, MASK_PRESENT_WIDTH);
                    self.stream.push_value(BINDING_KEPT, FLIP_WIDTH);
                    self.named_children(tile, nested);
                }
            }
            Node::PointList => {
                let size_offset = CELL_LEVEL - tile.level;
                self.stream.push_value(LEAF, LEAF_WIDTH);
                self.stream.push_value(BIND, CODE_WIDTH);
                self.stream.push_value(size_offset as u64, resolution_width(tile.level));
                self.stream.push_value(NO_MASKING, MASK_PRESENT_WIDTH);
                self.stream.push_value(POINT_LIST, PAYLOAD_MODE_WIDTH);
                point_list::write(self.bitmap, tile, self.stream);
            }
            Node::Unmasked { .. } | Node::Residual | Node::Absent => unreachable!("{node:?} is never written here"),
        }
    }

    /// A mask-present bit: whether the node masks some of what it holds.
    fn mask_present(&mut self, masks: bool) {
        self.stream.push_value(if masks { MASKING } else { NO_MASKING }, MASK_PRESENT_WIDTH);
    }

    /// A masking node's child mask -- each child a node of its own, or said
    /// by the node: left to the binding above, or copied -- then the
    /// children that are nodes.
    fn named_children(&mut self, tile: Tile, nested: &mut NestedResolutions) {
        let children = tile.children();
        let named = children.map(|child| self.tree.node(child) != Node::Absent);
        for is_named in named {
            self.stream.push_value(if is_named { MASKED } else { UNMASKED }, MASK_BIT_WIDTH);
        }
        for (child, is_named) in children.into_iter().zip(named) {
            if is_named {
                self.node(child, nested);
            }
        }
    }

    /// A complex tile's payload: the value of every tile of its resolution
    /// unmasked in it, a part at a time.
    fn payload(&mut self, tile: Tile, nesting: u8, size_offset: u8) {
        let resolution = tile.level + size_offset;
        for &part in self.payload_walk.parts(self.tree, tile, nesting) {
            write_part(self.bitmap, part, resolution, self.stream);
        }
    }
}

/// One part of a payload: the value of each of its tiles of
/// `resolution`, in Morton order. At 1x1, those are its cells, as they
/// lie in the bitmap: written a word at a time.
fn write_part(bitmap: &Bitmap, part: Tile, resolution: u8, stream: &mut BitStream) {
    if resolution == CELL_LEVEL {
        let cells = cells_in_tile(part.level) as usize;
        for word in bitmap.square_words(part.top_left_cell(), part.side_in_cells()) {
            stream.push_value(word, cells.min(u64::BITS as usize) as u8);
        }
        return;
    }
    for tile in part.tiles_at_size_offset(resolution - part.level) {
        stream.push(tile.top_left_value(bitmap));
    }
}
