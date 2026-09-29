//! A cell list: a tile's set cells said one by one, for a tile where
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

/// How many of `tile`'s cells are set, counted a word of cells at a time.
fn set_count(bitmap: &Bitmap, tile: Tile) -> u64 {
    bitmap.square_words(tile.top_left_cell(), tile.side_in_cells()).map(|word| word.count_ones() as u64).sum()
}

/// The bits `tile`'s cell list takes, without writing it.
pub fn bits(bitmap: &Bitmap, tile: Tile) -> u64 {
    let (_, parameter, fixed_bits) = count_and_fixed_bits(bitmap, tile);
    fixed_bits + gaps(bitmap, tile).map(|gap| gap >> parameter).sum::<u64>()
}

/// The bits `tile`'s cell list takes, if fewer than `limit`: counted no
/// further once it is certain they are not.
pub fn bits_below(bitmap: &Bitmap, tile: Tile, limit: u64) -> Option<u64> {
    let (set, parameter, mut bits) = count_and_fixed_bits(bitmap, tile);
    if bits + least_high_parts(bitmap, tile, set, parameter) >= limit {
        return None;
    }
    for gap in gaps(bitmap, tile) {
        bits += gap >> parameter;
        if bits >= limit {
            return None;
        }
    }
    Some(bits)
}

/// How many of `tile`'s cells are set, the Rice parameter of their gaps,
/// and the bits of `tile`'s cell list that do not depend on where they
/// are: the count, then every gap's unary end and low bits -- all but
/// each gap's high part.
fn count_and_fixed_bits(bitmap: &Bitmap, tile: Tile) -> (u64, u8, u64) {
    let set = set_count(bitmap, tile);
    let parameter = rice_parameter(cells_in_tile(tile.level), set);
    (set, parameter, gamma_bits(set + 1) + set * (1 + parameter as u64))
}

/// The least the high parts of `tile`'s `set` gaps can add up to, known
/// without reading each: the gaps add up to the cells before the last
/// set one that are clear, and each gap's high part is at least what
/// the gap is past its low part's largest value, shifted down. Exact when
/// `parameter` is 0.
fn least_high_parts(bitmap: &Bitmap, tile: Tile, set: u64, parameter: u8) -> u64 {
    let Some(last_set) = last_set_place(bitmap, tile) else { return 0 };
    let every_gap = last_set + 1 - set;
    let largest_low_part = (1 << parameter) - 1;
    every_gap.saturating_sub(set * largest_low_part) >> parameter
}

/// The place, in `tile`'s own Morton order, of its last set cell, if any.
fn last_set_place(bitmap: &Bitmap, tile: Tile) -> Option<u64> {
    let words = bitmap.square_words(tile.top_left_cell(), tile.side_in_cells());
    let last = words.enumerate().filter(|&(_, word)| word != 0).last()?;
    let (word_index, word) = last;
    Some((word_index * u64::BITS as usize + (u64::BITS - 1 - word.leading_zeros()) as usize) as u64)
}

/// Writes `tile`'s cell list.
pub fn write(bitmap: &Bitmap, tile: Tile, stream: &mut BitStream) {
    let set = set_count(bitmap, tile);
    write_gamma(set + 1, stream);
    let parameter = rice_parameter(cells_in_tile(tile.level), set);
    for gap in gaps(bitmap, tile) {
        write_unary(gap >> parameter, stream);
        stream.push_value(gap, parameter);
    }
}

/// Reads a cell list for `tile`, setting its cells in `cell_values`;
/// every other cell of the tile stays as it is.
pub fn read(reader: &mut BitReader, tile: Tile, cell_values: &mut Bitmap) {
    let set = read_gamma(reader) - 1;
    let parameter = rice_parameter(cells_in_tile(tile.level), set);
    let mut place = 0;
    for _ in 0..set {
        let gap = read_unary(reader) << parameter | reader.value(parameter);
        place += gap as usize;
        cell_values.set_in_square(tile.top_left_cell(), place);
        place += 1;
    }
}

/// The gaps between `tile`'s set cells: the cells skipped before each.
fn gaps(bitmap: &Bitmap, tile: Tile) -> impl Iterator<Item = u64> + '_ {
    let mut next = 0;
    bitmap.set_cells_in_square(tile.top_left_cell(), tile.side_in_cells()).map(move |place| {
        let gap = (place - next) as u64;
        next = place + 1;
        gap
    })
}

/// Writes `count` in unary: that many ones, then a zero.
fn write_unary(count: u64, stream: &mut BitStream) {
    for _ in 0..count {
        stream.push(true);
    }
    stream.push(false);
}

/// Reads what [`write_unary`] wrote.
fn read_unary(reader: &mut BitReader) -> u64 {
    let mut count = 0;
    while reader.bit() {
        count += 1;
    }
    count
}

/// Writes `value`, at least 1, in Elias gamma code: its length less one
/// in unary, then all but its top bit.
fn write_gamma(value: u64, stream: &mut BitStream) {
    let length = value.ilog2() as u8;
    write_unary(length as u64, stream);
    stream.push_value(value, length);
}

/// Reads what [`write_gamma`] wrote.
fn read_gamma(reader: &mut BitReader) -> u64 {
    let length = read_unary(reader) as u8;
    1 << length | reader.value(length)
}
