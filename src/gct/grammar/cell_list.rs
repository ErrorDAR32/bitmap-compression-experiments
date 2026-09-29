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

use super::bit_stream::{gamma_bits, BitReader, BitStream};
use crate::gct::tile::{cells_in_tile, Tile};
use crate::Bitmap;

/// The low bits of every gap, given `cells` in the tile and `set` of
/// them set: the whole part of log2 of the mean gap, `(cells - set) /
/// set`, so the unary high part is short on average.
fn rice_parameter(cells: u64, set: u64) -> u8 {
    let mean_gap = (cells - set) / set.max(1);
    mean_gap.checked_ilog2().unwrap_or(0) as u8
}

/// How many of `tile`'s cells are set, counted a word of cells at a time.
fn set_count(bitmap: &Bitmap, tile: Tile) -> u64 {
    bitmap.square_words(tile.top_left_cell(), tile.side_in_cells()).map(|word| word.count_ones() as u64).sum()
}

/// The fewest bits `tile`'s cell list could take: its count and every
/// gap's unary end and low bits, read off how many cells are set --
/// every gap's high part taken as nothing.
pub fn least_bits(bitmap: &Bitmap, tile: Tile) -> u64 {
    let set = set_count(bitmap, tile);
    let parameter = rice_parameter(cells_in_tile(tile.level), set);
    gamma_bits(set + 1) + set * (1 + parameter as u64)
}

/// The bits `tile`'s cell list takes, without writing it.
pub fn bits(bitmap: &Bitmap, tile: Tile) -> u64 {
    let set = set_count(bitmap, tile);
    let parameter = rice_parameter(cells_in_tile(tile.level), set);
    // Every gap's unary end and low bits, then each gap's high part: a
    // word of cells at a time, each set cell its place in the tile's
    // Morton order.
    let (mut high_parts, mut next) = (0, 0);
    for (word_index, word) in bitmap.square_words(tile.top_left_cell(), tile.side_in_cells()).enumerate() {
        let mut remaining = word;
        while remaining != 0 {
            let place = word_index * u64::BITS as usize + remaining.trailing_zeros() as usize;
            high_parts += ((place - next) as u64) >> parameter;
            next = place + 1;
            remaining &= remaining - 1;
        }
    }
    gamma_bits(set + 1) + set * (1 + parameter as u64) + high_parts
}

/// Writes `tile`'s cell list.
pub fn write(bitmap: &Bitmap, tile: Tile, stream: &mut BitStream) {
    let set = set_count(bitmap, tile);
    stream.push_gamma(set + 1);
    let parameter = rice_parameter(cells_in_tile(tile.level), set);
    for gap in gaps(bitmap, tile) {
        stream.push_unary(gap >> parameter);
        stream.push_value(gap, parameter);
    }
}

/// Reads a cell list for `tile`, setting its cells in `cell_values`;
/// every other cell of the tile stays as it is.
pub fn read(reader: &mut BitReader, tile: Tile, cell_values: &mut Bitmap) {
    let set = reader.gamma() - 1;
    let parameter = rice_parameter(cells_in_tile(tile.level), set);
    let mut place = 0;
    for _ in 0..set {
        let gap = reader.unary() << parameter | reader.value(parameter);
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
