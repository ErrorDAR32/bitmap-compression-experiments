//! Decoding: read the stream back by the grammar
//! ([`crate::gct::grammar`]) -- the tree, and every cell the stream binds
//! outright -- then resolve the cells copies cover.
//!
//! A copy is chosen on content alone, so its source may not be resolved
//! yet when the tree reaches it -- it may even be a residual cell the
//! residual pass binds. So copies are resolved last, a 4x4 block at a
//! time: every copy's own cells are whole 4x4 blocks, and a block's
//! source is the block the copy's offset away. Reading the tree notes
//! each copied block's source block; then the blocks are copied in
//! Morton order, each one's 16 cells a single run of the bitmap. A
//! source above and to the right comes later in Morton order, and may be
//! a copied block itself not copied yet: then its own source is copied
//! first, and so on down the chain. Every source is strictly earlier in
//! the blocks' reading order -- above, or to the left in the same row --
//! so a chain always ends.

use crate::gct::grammar::bit_stream::{BitReader, BitStream};
use crate::gct::grammar::order::PayloadWalk;
use crate::gct::grammar::point_list;
use crate::gct::grammar::*;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::fixed_list::FixedList;
use crate::gct::pyramids::copyable::CopyOffsets;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{cells_in_tile, tiles_across, Tile, CELL_LEVEL};
use crate::gct::pyramids::copy_sources::{CopySources, BLOCKS, BLOCK_LEVEL};
use crate::morton::morton_index;
use crate::Bitmap;

/// Where reading a stream back writes: the tree, and every cell whose
/// value the stream binds outright -- all but the cells copies cover.
pub struct StreamContents<'a> {
    /// The tree the stream spells out.
    pub tree: &'a mut Pyramid,
    /// Every cell's value, where the stream binds it outright; clear
    /// where a copy covers it, until copies are resolved.
    pub cell_values: &'a mut Bitmap,
    /// The blocks copies cover, and where each copies from.
    pub copies: &'a mut Copies,
    /// Where copies read from: the offsets the stream was encoded with.
    pub offsets: &'a CopyOffsets,
}

impl StreamContents<'_> {
    /// Binds every cell of `tile` to `value`.
    fn bind(&mut self, tile: Tile, value: bool) {
        if value {
            tile.set_in(self.cell_values);
        }
    }
}

/// Reads back what [`crate::gct::encode::write`] wrote, into `read`,
/// whatever it held before; `payload_walk` is room for a payload's parts.
pub fn read(stream: &BitStream, read: &mut StreamContents, payload_walk: &mut PayloadWalk) {
    read.tree.clear();
    read.cell_values.reset();
    let mut reader = stream.reader();
    let start_level = reader.value(START_LEVEL_WIDTH) as u8;
    for level in 0..start_level {
        for tile in Tile::all_of_level(level) {
            read.tree.set_node(tile, Node::Subdivided);
        }
    }
    for tile in Tile::all_of_level(start_level) {
        read_node(&mut reader, tile, &mut NestedResolutions::none(), BOUND_AT_THE_TOP, read, payload_walk);
    }
    for square in read.tree.residual_squares() {
        let cells = reader.value(RESIDUAL_SQUARE_BITS);
        read.cell_values.set_in_small_square(square.top_left_cell(), square.side_in_cells(), cells);
    }
}

/// Reads `tile`'s node and everything under it, `bound_above` the value
/// bound above it.
fn read_node(
    reader: &mut BitReader,
    tile: Tile,
    nested: &mut NestedResolutions,
    bound_above: bool,
    read: &mut StreamContents,
    payload_walk: &mut PayloadWalk,
) {
    for nesting in nested.able_to_unmask(tile) {
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
        read_payload(reader, tile, nested.next_nesting(), 0, read, payload_walk);
        return;
    }
    if !leaf {
        if !divide_may_mask(tile.level) || reader.value(MASK_PRESENT_WIDTH) == NO_MASKING {
            read.tree.set_node(tile, Node::Subdivided);
            for child in tile.children() {
                read_node(reader, child, nested, bound_above, read, payload_walk);
            }
            return;
        }
        let flips = reader.value(FLIP_WIDTH) == BINDING_FLIPPED;
        let bound_above = bound_above != flips;
        let node = if flips { Node::ComplexTile { size_offset: 0, masks: true } } else { Node::Subdivided };
        read.tree.set_node(tile, node);
        let children = tile.children();
        let named = children.map(|_| reader.value(MASK_BIT_WIDTH) == MASKED);
        for (child, is_named) in children.into_iter().zip(named) {
            if is_named {
                read_node(reader, child, nested, bound_above, read, payload_walk);
            } else {
                read.bind(child, bound_above);
            }
        }
        return;
    }
    if reader.value(CODE_WIDTH) == COPY {
        let far = reader.value(FAR_WIDTH) != 0;
        let direction = reader.value(DIRECTION_WIDTH) as u8;
        let masks = copy_may_mask(tile.level) && reader.value(MASK_PRESENT_WIDTH) == MASKING;
        read.tree.set_node(tile, Node::Copied { far, direction, masks });
        if !masks {
            read.copies.cover(tile, tile, read.offsets.offset(far, direction));
            return;
        }
        let children = tile.children();
        let masked = children.map(|_| reader.value(MASK_BIT_WIDTH) == MASKED);
        for (child, is_masked) in children.into_iter().zip(masked) {
            if is_masked {
                read_node(reader, child, nested, bound_above, read, payload_walk);
            } else {
                read.copies.cover(tile, child, read.offsets.offset(far, direction));
            }
        }
        return;
    }
    let size_offset = reader.value(resolution_width(tile.level)) as u8;
    let masks = complex_tile_may_mask(tile.level, size_offset) && reader.value(MASK_PRESENT_WIDTH) == MASKING;
    if has_payload_mode(tile.level, size_offset, masks) && reader.value(PAYLOAD_MODE_WIDTH) == POINT_LIST {
        read.tree.set_node(tile, Node::PointList);
        read.bind(tile, false);
        point_list::read(reader, tile, read.cell_values);
        return;
    }
    read.tree.set_node(tile, Node::ComplexTile { size_offset, masks });
    let nesting = nested.next_nesting();
    if masks {
        nested.while_nested(tile.level + size_offset, |inside| {
            for child in tile.children() {
                read_node(reader, child, inside, bound_above, read, payload_walk);
            }
        });
    }
    read_payload(reader, tile, nesting, size_offset, read, payload_walk);
}

/// A complex tile's payload, bound into the cells of the tiles it names,
/// a part at a time.
fn read_payload(reader: &mut BitReader, tile: Tile, nesting: u8, size_offset: u8, read: &mut StreamContents, payload_walk: &mut PayloadWalk) {
    let resolution = tile.level + size_offset;
    for &part in payload_walk.parts(read.tree, tile, nesting) {
        read_part(reader, part, resolution, read.cell_values);
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

/// Cells a block: one run of the bitmap, in Morton order.
const BLOCK_CELLS: usize = cells_in_tile(BLOCK_LEVEL) as usize;

/// Room to resolve copies in, allocated once at the most there are.
pub struct Copies {
    /// Each block's source: a [copy sources
    /// pyramid](crate::gct::pyramids::copy_sources).
    sources: Pyramid,
    /// The blocks copies cover, in the order the tree names them --
    /// Morton order.
    covered: FixedList<Tile, BLOCKS>,
    /// Blocks waiting on their source to be copied first: a chain, never
    /// longer than there are blocks.
    waiting: FixedList<Tile, BLOCKS>,
}

impl Default for Copies {
    /// Nothing covered.
    fn default() -> Self {
        Self { sources: Pyramid::copy_sources(), covered: FixedList::new(), waiting: FixedList::new() }
    }
}

impl Copies {
    /// Forgets every block covered.
    fn clear(&mut self) {
        self.sources.clear();
        self.covered.clear();
    }

    /// Notes that `part` -- the copy at `copy`, or a child of it the copy
    /// says itself -- is copied from `far` away in `direction`: each of
    /// its blocks from the block the copy's offset away.
    fn cover(&mut self, copy: Tile, part: Tile, (dx, dy): (isize, isize)) {
        let reach = tiles_across(BLOCK_LEVEL - copy.level) as isize;
        for block in part.tiles_at_size_offset(BLOCK_LEVEL - part.level) {
            let (x, y) = ((block.x as isize + dx * reach) as u8, (block.y as isize + dy * reach) as u8);
            self.sources.set_source(block, Tile { level: BLOCK_LEVEL, x, y });
            self.covered.push(block);
        }
    }

    /// Copies every covered block's cells in `cells`, in Morton order,
    /// each block's source first when that is a covered block not copied
    /// yet.
    fn resolve(&mut self, cells: &mut Bitmap) {
        for at in 0..self.covered.len() {
            self.waiting.push(self.covered[at]);
            while let Some(&block) = self.waiting.last() {
                let Some(source) = self.sources.source_of(block) else {
                    self.waiting.pop();
                    continue;
                };
                if self.sources.source_of(source).is_some() {
                    self.waiting.push(source);
                    continue;
                }
                let run = cells.morton_run(morton_index(source.x, source.y) * BLOCK_CELLS, BLOCK_CELLS);
                cells.set_in_morton_run(morton_index(block.x, block.y) * BLOCK_CELLS, BLOCK_CELLS, run);
                self.sources.mark_copied(block);
                self.waiting.pop();
            }
        }
    }
}

/// Decodes a stream written by [`crate::gct::encode::write`] into
/// `read`, whose cell values end as the bitmap; `payload_walk` is room for a
/// payload's parts.
pub fn decode(stream: &BitStream, read: &mut StreamContents, payload_walk: &mut PayloadWalk) {
    read.copies.clear();
    self::read(stream, read, payload_walk);
    read.copies.resolve(read.cell_values);
}
