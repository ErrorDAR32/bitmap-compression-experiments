//! A point list: a tile's set cells said one by one, for a tile where
//! few are set -- the payload of a complex tile of 1x1 resolution that
//! masks nothing, when cheaper than saying every cell raw.
//!
//! How many cells are set, `k`, in Elias gamma code (of `k + 1`, so zero
//! can be said); then each set cell's place in the tile's own Morton
//! order, as the gap since the last one -- the cells skipped -- in Rice
//! code: the gap's high part in unary, then as many of its low bits as
//! the Rice parameter says. The parameter follows from the tile's cell
//! count and `k`, so it is never written. That comes to about
//! `k * (log2(cells / k) + 1.5)` bits: near what `k` cells scattered at
//! random need at least.

use super::bit_stream::{BitReader, BitStream};
use crate::gct::tile::{cells_in_tile, Tile};
use crate::Bitmap;

/// The low bits of every gap, given `cells` in the tile and `set` of
/// them set: the whole part of log2 of the mean gap, `(cells - set) /
/// set`, so the unary high part is short on average.
fn rice_parameter(cells: u64, set: u64) -> u8 {
    let mean_gap = (cells - set) / set.max(1);
    mean_gap.checked_ilog2().unwrap_or(0) as u8
}

/// The bits the Elias gamma code of `value`, at least 1, takes: its
/// length less one in unary, then all but its top bit.
fn gamma_bits(value: u64) -> u64 {
    2 * value.ilog2() as u64 + 1
}

/// The bits `tile`'s point list takes: counted a word of cells at a
/// time, since it is asked of every tile that could be one.
pub fn bits(bitmap: &Bitmap, tile: Tile) -> u64 {
    let cells = cells_in_tile(tile.level);
    let words = || bitmap.square_words(tile.top_left_cell(), tile.side_in_cells());
    let set = words().map(|word| word.count_ones() as u64).sum::<u64>();
    let parameter = rice_parameter(cells, set);
    // Every gap's unary end and low bits, then each gap's high part.
    let mut bits = gamma_bits(set + 1) + set * (1 + parameter as u64);
    let mut next = 0;
    for (at, mut word) in words().enumerate() {
        while word != 0 {
            let offset = (at * u64::BITS as usize + word.trailing_zeros() as usize) as u64;
            bits += (offset - next) >> parameter;
            next = offset + 1;
            word &= word - 1;
        }
    }
    bits
}

/// Writes `tile`'s point list.
pub fn write(bitmap: &Bitmap, tile: Tile, out: &mut BitStream) {
    let cells = cells_in_tile(tile.level);
    let set = set_cells(bitmap, tile).count() as u64;
    write_gamma(set + 1, out);
    let parameter = rice_parameter(cells, set);
    for gap in gaps(bitmap, tile) {
        for _ in 0..gap >> parameter {
            out.push(true);
        }
        out.push(false);
        out.push_value(gap, parameter);
    }
}

/// Reads a point list for `tile`, setting its cells in `cell_values`;
/// every other cell of the tile stays as it is.
pub fn read(reader: &mut BitReader, tile: Tile, cell_values: &mut Bitmap) {
    let cells = cells_in_tile(tile.level);
    let set = read_gamma(reader) - 1;
    let parameter = rice_parameter(cells, set);
    let mut next = 0;
    for _ in 0..set {
        let mut high = 0;
        while reader.bit() {
            high += 1;
        }
        let gap = high << parameter | reader.value(parameter);
        let offset = next + gap as usize;
        cell_values.set_in_square(tile.top_left_cell(), offset);
        next = offset + 1;
    }
}

/// `tile`'s set cells, as places in its own Morton order.
fn set_cells(bitmap: &Bitmap, tile: Tile) -> impl Iterator<Item = usize> + '_ {
    bitmap.set_cells_in_square(tile.top_left_cell(), tile.side_in_cells())
}

/// The gaps between `tile`'s set cells: the cells skipped before each.
fn gaps(bitmap: &Bitmap, tile: Tile) -> impl Iterator<Item = u64> + '_ {
    let mut next = 0;
    set_cells(bitmap, tile).map(move |offset| {
        let gap = (offset - next) as u64;
        next = offset + 1;
        gap
    })
}

/// Writes `value`, at least 1, in Elias gamma code: its length less one
/// in unary, then all but its top bit.
fn write_gamma(value: u64, out: &mut BitStream) {
    let length = value.ilog2() as u8;
    for _ in 0..length {
        out.push(true);
    }
    out.push(false);
    out.push_value(value, length);
}

/// Reads what [`write_gamma`] wrote.
fn read_gamma(reader: &mut BitReader) -> u64 {
    let mut length = 0;
    while reader.bit() {
        length += 1;
    }
    1 << length | reader.value(length)
}
