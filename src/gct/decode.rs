//! Decoding: read the stream back by the grammar
//! ([`crate::gct::grammar`]) -- the tree, and every cell the stream binds
//! outright -- then resolve the cells copies cover.
//!
//! A copy is chosen on content alone, so its source may not be resolved
//! yet when the tree reaches it -- it may even be a residual cell the
//! residual pass binds. So copies are resolved last. Every copied cell's
//! source is before it in reading order -- above it, or to its left in
//! the same row -- so one pass in reading order resolves them all: each
//! copy's own cells, row by row, from the source rows the same offset
//! away.

use crate::gct::grammar::bit_stream::{BitReader, BitStream};
use crate::gct::grammar::order::Runs;
use crate::gct::grammar::point_list;
use crate::gct::grammar::*;
use crate::gct::nested_resolutions::NestedResolutions;
use crate::fixed_list::FixedList;
use crate::gct::pyramids::copyable::{FAR_DISTANCE, FINEST_COPY_LEVEL, NEAR_DISTANCE};
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{tile_side, Tile, CELLS, CELL_LEVEL, DIRECTIONS};
use crate::Bitmap;

/// Where reading a stream back writes: the tree, and every cell whose
/// value the stream binds outright -- all but the cells copies cover.
pub struct StreamContents<'a> {
    /// The tree the stream spells out.
    pub tree: &'a mut Pyramid,
    /// Every cell's value, where the stream binds it outright; clear
    /// where a copy covers it, until copies are resolved.
    pub cell_values: &'a mut Bitmap,
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
/// whatever it held before; `runs` is room for the runs' tiles.
pub fn read(stream: &BitStream, read: &mut StreamContents, runs: &mut Runs) {
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
        read_node(&mut reader, tile, &mut NestedResolutions::none(), BOUND_AT_THE_TOP, read, runs);
    }
    for &cell in runs.residual_cells(read.tree) {
        read.bind(cell, reader.bit());
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
    runs: &mut Runs,
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
        read_payload(reader, tile, nested.next_nesting(), 0, read, runs);
        return;
    }
    if !leaf {
        if !divide_may_mask(tile.level) || reader.value(MASK_PRESENT_WIDTH) == NO_MASKING {
            read.tree.set_node(tile, Node::Subdivided);
            for child in tile.children() {
                read_node(reader, child, nested, bound_above, read, runs);
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
                read_node(reader, child, nested, bound_above, read, runs);
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
        if masks {
            let children = tile.children();
            let masked = children.map(|_| reader.value(MASK_BIT_WIDTH) == MASKED);
            for (child, is_masked) in children.into_iter().zip(masked) {
                if is_masked {
                    read_node(reader, child, nested, bound_above, read, runs);
                }
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
                read_node(reader, child, inside, bound_above, read, runs);
            }
        });
    }
    read_payload(reader, tile, nesting, size_offset, read, runs);
}

/// A complex tile's payload, bound into the cells of the tiles it names.
fn read_payload(reader: &mut BitReader, tile: Tile, nesting: u8, size_offset: u8, read: &mut StreamContents, runs: &mut Runs) {
    for &part in runs.payload(read.tree, tile, nesting, size_offset) {
        read.bind(part, reader.bit());
    }
}

/// One row of a copy's own cells: `length` cells from `(x, y)`, each
/// copied from the cell `offset` away.
#[derive(Clone, Copy, Default)]
struct CopyRow {
    /// The row.
    y: u8,
    /// Its first cell's column.
    x: u8,
    /// How many cells.
    length: u16,
    /// Where its source is, in cells across and down.
    offset: (isize, isize),
}

/// The most rows copies cover: a copy's own cells are its tile, or a
/// child of it -- 4x4 at the least -- so every row holds at least four
/// cells.
const MOST_COPY_ROWS: usize = CELLS / tile_side(FINEST_COPY_LEVEL);

/// Room for the rows copies cover, allocated once at the most there are.
#[derive(Default)]
pub struct CopyRows(
    /// The rows, in reading order once sorted.
    FixedList<CopyRow, MOST_COPY_ROWS>,
);

/// Decodes a stream written by [`crate::gct::encode::write`] into
/// `read`, whose cell values end as the bitmap; `runs` and `copies` are
/// room for the runs' tiles and the copied rows.
pub fn decode(stream: &BitStream, read: &mut StreamContents, runs: &mut Runs, copies: &mut CopyRows) {
    self::read(stream, read, runs);
    copies.0.clear();
    copied_rows(read.tree, Tile::whole_bitmap(), &mut copies.0);
    copies.0.sort_unstable_by_key(|row| (row.y, row.x));
    let cells = &mut *read.cell_values;
    for row in copies.0.iter() {
        let source_y = (row.y as isize + row.offset.1) as u8;
        for x in row.x as usize..row.x as usize + row.length as usize {
            if cells.get((x as isize + row.offset.0) as u8, source_y) {
                cells.set(x as u8, row.y);
            }
        }
    }
}

/// Adds the rows of every copy's own cells at or under `tile`: the whole
/// copy, or, for a copy that masks, the children it says itself. A
/// masked child is a node of its own, walked like any other.
fn copied_rows(tree: &Pyramid, tile: Tile, rows: &mut FixedList<CopyRow, MOST_COPY_ROWS>) {
    match tree.node(tile) {
        Node::Copied { far, direction, masks } => {
            let distance = if far { FAR_DISTANCE } else { NEAR_DISTANCE };
            let (dx, dy) = DIRECTIONS[direction as usize];
            let reach = (tile_side(tile.level) * distance) as isize;
            let offset = (dx * reach, dy * reach);
            if !masks {
                add_rows(tile, offset, rows);
                return;
            }
            for child in tile.children() {
                if tree.node(child) == Node::Absent {
                    add_rows(child, offset, rows);
                } else {
                    copied_rows(tree, child, rows);
                }
            }
        }
        Node::Subdivided | Node::ComplexTile { masks: true, .. } => {
            for child in tile.children() {
                copied_rows(tree, child, rows);
            }
        }
        _ => {}
    }
}

/// Adds one row for each row of `tile`, copied from `offset` away.
fn add_rows(tile: Tile, offset: (isize, isize), rows: &mut FixedList<CopyRow, MOST_COPY_ROWS>) {
    let (left, top, _, bottom) = tile.cell_rect();
    let length = tile.side_in_cells() as u16;
    rows.extend((top..=bottom).map(|y| CopyRow { y, x: left, length, offset }));
}
