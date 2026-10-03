//! Tiles: a Morton word turned into rows and back, and windows at any
//! cell put together from four tiles, checked cell by cell against the
//! Morton index and the coordinates.
//!
//! `cargo test`

use bitmap::morton::morton_index;
use bitmap::window::{left_columns, morton_from_rows, rows_from_morton, top_rows, window, WORD_TILE_SIDE};

/// Words drawn from a seed: SplitMix64, enough for a test.
struct Rng(u64);

impl Rng {
    /// Draws from `seed`.
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next word.
    fn draw(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let z = (self.0 ^ (self.0 >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Whether the cell `(x, y)` of a row-by-row tile is set.
fn at(rows: u64, x: u32, y: u32) -> bool {
    rows >> (y * WORD_TILE_SIDE + x) & 1 == 1
}

/// A Morton word's cell `(x, y)` lands at bit `y * 8 + x` of its rows,
/// and back.
#[test]
fn morton_words_turn_into_rows_and_back() {
    let mut random = Rng::new(7);
    for _ in 0..1000 {
        let word = random.draw();
        let rows = rows_from_morton(word);
        for (x, y) in (0..8).flat_map(|y| (0..8).map(move |x| (x, y))) {
            assert_eq!(at(rows, x, y), word >> morton_index(x as u8, y as u8) & 1 == 1, "({x}, {y})");
        }
        assert_eq!(morton_from_rows(rows), word);
    }
}

/// A window at any offset into four tiles is the 8x8 cells there.
#[test]
fn windows_cut_the_cells_from_four_tiles() {
    let mut random = Rng::new(9);
    for _ in 0..200 {
        let tiles = [[random.draw(), random.draw()], [random.draw(), random.draw()]];
        let cell = |x: u32, y: u32| at(tiles[(y / 8) as usize][(x / 8) as usize], x % 8, y % 8);
        for (across, down) in (0..8).flat_map(|down| (0..8).map(move |across| (across, down))) {
            let cut = window(tiles, across, down);
            for (x, y) in (0..8).flat_map(|y| (0..8).map(move |x| (x, y))) {
                assert_eq!(at(cut, x, y), cell(across + x, down + y), "window at ({across}, {down}), cell ({x}, {y})");
            }
        }
    }
}

/// The masks keep what they say.
#[test]
fn masks_keep_columns_and_rows() {
    assert_eq!((left_columns(0), left_columns(8), left_columns(3) & 0xff), (0, u64::MAX, 0b111));
    assert_eq!((top_rows(0), top_rows(8), top_rows(2)), (0, u64::MAX, 0xffff));
}
