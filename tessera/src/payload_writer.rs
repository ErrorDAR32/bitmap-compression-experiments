//! Complex tiles' payloads, written and read: a value for every tile of
//! the resolution, or, at 1x1, a cell list of the set cells.
//! `docs/tessera.md`, "Payloads and cell lists".
//!
//! Function by function: `docs/reference.md`, "`payload_writer.rs`".

use crate::bit_stream::{gamma_bits, BitReader, Sink};
use crate::tile::{cells_in_tile, tiles_in_level, Tile, CELL_LEVEL};
use bitmap::Bitmap;

/// A complex tile's payload: the value of each of `tile`'s tiles
/// `size_offset` levels finer, in Morton order -- at 1x1, its cells as
/// they lie in the bitmap, a word at a time.
pub fn write_payload(sink: &mut impl Sink, bitmap: &Bitmap, tile: Tile, size_offset: u8) {
    if let Some(bits) = sink.counted() {
        *bits += tiles_in_level(size_offset) as u64;
        return;
    }
    if tile.level + size_offset == CELL_LEVEL {
        let width = cells_in_tile(tile.level).min(u64::BITS as usize) as u8;
        for word in bitmap.tile_words(tile.top_left_cell(), tile.side_in_cells()) {
            sink.push_value(word, width);
        }
        return;
    }
    for resolution_tile in tile.tiles_under(size_offset) {
        sink.push(resolution_tile.top_left_value(bitmap));
    }
}

/// Reads what [`write_payload`] wrote into `cells`.
pub fn read_payload(reader: &mut BitReader, cells: &mut Bitmap, tile: Tile, size_offset: u8) {
    let (corner, side) = (tile.top_left_cell(), tile.side_in_cells());
    if tile.level + size_offset != CELL_LEVEL {
        for resolution_tile in tile.tiles_under(size_offset) {
            if reader.bit() {
                cells.set_tile(resolution_tile.top_left_cell(), resolution_tile.side_in_cells());
            }
        }
    } else if side * side < u64::BITS as usize {
        cells.set_in_small_tile(corner, side, reader.value((side * side) as u8));
    } else {
        for word in cells.tile_words_mut(corner, side) {
            *word |= reader.value(u64::BITS as u8);
        }
    }
}

/// The low bits of every gap, given `cells` in the tile and `set` of
/// them set: the whole part of log2 of the mean gap, `(cells - set) /
/// set`, so the unary high part is short on average.
fn rice_parameter(cells: u64, set: u64) -> u8 {
    let mean_gap = (cells - set) / set.max(1);
    mean_gap.checked_ilog2().unwrap_or(0) as u8
}

/// The fewest bits the cell list of a tile of `level`, `set` of its
/// cells set, could take: its count and every gap's unary end and low
/// bits -- every gap's high part taken as nothing.
pub fn cell_list_least_bits(level: u8, set: u64) -> u64 {
    let parameter = rice_parameter(cells_in_tile(level) as u64, set);
    gamma_bits(set + 1) + set * (1 + parameter as u64)
}

/// Writes `tile`'s cell list.
pub fn write_cell_list(sink: &mut impl Sink, bitmap: &Bitmap, tile: Tile) {
    let (corner, side) = (tile.top_left_cell(), tile.side_in_cells());
    let set = bitmap.tile_words(corner, side).map(|word| word.count_ones() as u64).sum();
    sink.push_gamma(set + 1);
    let parameter = rice_parameter(cells_in_tile(tile.level) as u64, set);
    let mut next = 0;
    for place in bitmap.set_cells_in_tile(corner, side) {
        let gap = (place - next) as u64;
        sink.push_unary(gap >> parameter);
        sink.push_value(gap, parameter);
        next = place + 1;
    }
}

/// Reads a cell list for `tile`, setting its cells in `cell_values`;
/// every other cell of the tile stays as it is.
pub fn read_cell_list(reader: &mut BitReader, tile: Tile, cell_values: &mut Bitmap) {
    let set = reader.gamma() - 1;
    let parameter = rice_parameter(cells_in_tile(tile.level) as u64, set);
    let mut place = 0;
    for _ in 0..set {
        let gap = reader.unary() << parameter | reader.value(parameter);
        place += gap as usize;
        cell_values.set_in_tile(tile.top_left_cell(), place);
        place += 1;
    }
}
