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

use super::bit_stream::{gamma_bits, BitReader, Sink};
use crate::tile::{cells_in_tile, Tile};
use bitmap::Bitmap;

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
pub fn least_bits(level: u8, set: u64) -> u64 {
    let parameter = rice_parameter(cells_in_tile(level) as u64, set);
    gamma_bits(set + 1) + set * (1 + parameter as u64)
}

/// Writes `tile`'s cell list.
pub fn write(sink: &mut impl Sink, bitmap: &Bitmap, tile: Tile) {
    let (corner, side) = (tile.top_left_cell(), tile.side_in_cells());
    let set = bitmap.square_words(corner, side).map(|word| word.count_ones() as u64).sum();
    sink.push_gamma(set + 1);
    let parameter = rice_parameter(cells_in_tile(tile.level) as u64, set);
    let mut next = 0;
    for place in bitmap.set_cells_in_square(corner, side) {
        let gap = (place - next) as u64;
        sink.push_unary(gap >> parameter);
        sink.push_value(gap, parameter);
        next = place + 1;
    }
}

/// Reads a cell list for `tile`, setting its cells in `cell_values`;
/// every other cell of the tile stays as it is.
pub fn read(reader: &mut BitReader, tile: Tile, cell_values: &mut Bitmap) {
    let set = reader.gamma() - 1;
    let parameter = rice_parameter(cells_in_tile(tile.level) as u64, set);
    let mut place = 0;
    for _ in 0..set {
        let gap = reader.unary() << parameter | reader.value(parameter);
        place += gap as usize;
        cell_values.set_in_square(tile.top_left_cell(), place);
        place += 1;
    }
}
