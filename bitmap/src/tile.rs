//! Tiles: 8x8 squares of cells, a `u64` each. In Morton order an aligned
//! 8x8 square is one word of a bitmap, so a tile is read as one load; to
//! shift and cut it, it is turned row by row -- bit `y * 8 + x`, as a
//! chess board -- where moving cells across is a shift and keeping
//! columns a mask. A window of 8x8 cells at any cell is then put
//! together from the up to four aligned tiles it overlaps, in a few
//! shifts and masks.
//!
//! Turning a Morton word into rows reorders its index bits -- `x0 y0 x1
//! y1 x2 y2`, from the lowest, to `x0 x1 x2 y0 y1 y2` -- in three
//! exchanges of two index bits, each a delta swap of the word's bits.

/// Cells along a tile's side.
pub const TILE_SIDE: u32 = 8;

/// The bits of a word whose index has bit `low` set and bit `high`
/// clear: what a delta swap of those two index bits moves up.
const fn swap_mask(low: u32, high: u32) -> u64 {
    let mut mask = 0u64;
    let mut position = 0;
    while position < u64::BITS {
        if position >> low & 1 == 1 && position >> high & 1 == 0 {
            mask |= 1 << position;
        }
        position += 1;
    }
    mask
}

/// `word` with its bits' index bits `low` and `high` exchanged: the bits
/// at [`swap_mask`] and those `2^high - 2^low` above them traded.
const fn swap_index_bits(word: u64, low: u32, high: u32) -> u64 {
    let delta = (1 << high) - (1 << low);
    let mask = swap_mask(low, high);
    let traded = ((word >> delta) ^ word) & mask;
    word ^ traded ^ (traded << delta)
}

/// The index bits exchanged, in order, turning Morton order into rows.
const MORTON_TO_ROWS: [(u32, u32); 3] = [(1, 2), (2, 4), (3, 4)];

/// An aligned 8x8 tile, one Morton-ordered word of a bitmap, row by row:
/// cell `(x, y)` of the tile at bit `y * 8 + x`.
pub const fn rows_from_morton(word: u64) -> u64 {
    let mut rows = word;
    let mut step = 0;
    while step < MORTON_TO_ROWS.len() {
        rows = swap_index_bits(rows, MORTON_TO_ROWS[step].0, MORTON_TO_ROWS[step].1);
        step += 1;
    }
    rows
}

/// [`rows_from_morton`] undone: a tile's rows as a Morton-ordered word.
pub const fn morton_from_rows(rows: u64) -> u64 {
    let mut word = rows;
    let mut step = MORTON_TO_ROWS.len();
    while step > 0 {
        step -= 1;
        word = swap_index_bits(word, MORTON_TO_ROWS[step].0, MORTON_TO_ROWS[step].1);
    }
    word
}

/// The first `columns` columns of every row (0 to 8).
pub const fn left_columns(columns: u32) -> u64 {
    let row = ((1u16 << columns) - 1) as u64;
    row * 0x0101_0101_0101_0101
}

/// The first `rows` rows (0 to 8).
pub const fn top_rows(rows: u32) -> u64 {
    if rows >= TILE_SIDE { u64::MAX } else { (1 << (rows * TILE_SIDE)) - 1 }
}

/// The 8x8 window, row by row, whose top left cell is `(across, down)`
/// -- each 0 to 7 -- in the 16x16 square made of four aligned tiles, row
/// by row: `[[top left, top right], [bottom left, bottom right]]`.
pub const fn window(tiles: [[u64; 2]; 2], across: u32, down: u32) -> u64 {
    let top = beside(tiles[0][0], tiles[0][1], across);
    let bottom = beside(tiles[1][0], tiles[1][1], across);
    let below = match bottom.checked_shl(TILE_SIDE * (TILE_SIDE - down)) {
        Some(below) => below,
        None => 0,
    };
    top >> (TILE_SIDE * down) | below
}

/// The 8x8 window `across` columns into two tiles side by side, row by
/// row: the left one's columns from `across` on, then the right one's
/// first `across`.
const fn beside(left: u64, right: u64, across: u32) -> u64 {
    let kept = left_columns(TILE_SIDE - across);
    (left >> across) & kept | (right << (TILE_SIDE - across)) & !kept
}
