//! The count split: what the stream says instead of the tree for a
//! bitmap it [suits] -- sparse cells, clustered, with nothing to
//! copy: the tree's worst case. Only one of the two is ever made: which
//! is judged from the patterns pyramid, before either.
//!
//! How many cells are set, `k`, in Elias gamma code (of `k + 1`, so zero
//! can be said); then the bitmap's cells in Morton order, halved again
//! and again. Every run holding some set cells and some clear says how
//! many of its set cells lie in its first half: one of the counts its
//! halves could hold between them, each taken as equally likely, in
//! truncated binary. A run all set or all clear says nothing more, and
//! nothing inside it is said: an empty region costs nothing, and a run
//! holding one set cell a bit a halving. Halving the Morton order splits
//! a square into two rectangles, and each of those into two squares, so
//! the runs are the regions of a binary partition of the plane.

use super::bit_stream::{gamma_bits, truncated_binary_bits, BitReader, BitStream};
use crate::gct::pyramids::patterns::{Patterns, FINEST_NUMBERED_LEVEL};
use crate::gct::tile::{cells_in_tile, tiles_in_level, CELLS};
use crate::Bitmap;

/// The counts a run of `cells` cells, `set` of them set, could have in
/// its first half: the fewest, and how many there are.
fn first_half_counts(cells: usize, set: u64) -> (u64, u64) {
    let half = (cells / 2) as u64;
    let fewest = set.saturating_sub(half);
    (fewest, set.min(half) - fewest + 1)
}

/// Visits every run of the Morton order from `first`, `cells` long,
/// `set` of them set, that says something, depth first: `say` gets how
/// many of its set cells lie in its first half, the fewest that could,
/// and how many counts could.
fn walk(bitmap: &Bitmap, first: usize, cells: usize, set: u64, say: &mut impl FnMut(u64, u64, u64)) {
    if set == 0 || set == cells as u64 {
        return;
    }
    let half = cells / 2;
    let first_half_set = bitmap.count_in_morton_block(first, half);
    let (fewest, counts) = first_half_counts(cells, set);
    say(first_half_set, fewest, counts);
    walk(bitmap, first, half, first_half_set, say);
    walk(bitmap, first + half, half, set - first_half_set, say);
}

/// The most cells a bitmap can have set for the count split to suit it:
/// an eighth. It is the fallback for sparse cells.
const MOST_SET: u64 = (CELLS / 8) as u64;

/// The most cells a bitmap can have set for the count split to suit it
/// however they lie: a thousandth, 64. So few cells take fewer bits
/// counted and split than as a tree, scattered or clustered alike: at
/// 6 scattered cells 94 bits against 124, at 65 about 786 against 802,
/// and at 196 the tree wins (`sparse.csv`).
const MOST_SET_ALWAYS_SUITED: u64 = (CELLS / 1024) as u64;

/// How much more clustered than random cells a bitmap's set cells must be
/// for the count split to suit it: random cells at its density would
/// occupy a fifth more of its 4x4 tiles than they do. Scattered cells
/// measure within a few percent of random, and the tree says them in
/// fewer bits; a few bunched groups measure three to ten times random,
/// and the count split says them in 9-21% fewer (`sparse.csv`).
const LEAST_CLUSTERING: f64 = 1.2;

/// The fewest distinct patterns, for each occupied 4x4 tile, a bitmap
/// must hold for the count split to suit it. Cells that repeat -- lines,
/// streets -- measure under a sixth: the tree copies them, and says them
/// in a third to a half of the count split's bits. Grown clusters
/// measure over two fifths.
const LEAST_DISTINCT_A_TILE: f64 = 0.25;

/// Whether the count split suits `bitmap` better than the tree, judged
/// from its patterns pyramid alone, before either is made: a bitmap at
/// most a thousandth set, or one at most an eighth set whose set cells
/// are clustered and do not repeat.
pub fn suits(bitmap: &Bitmap, patterns: &Patterns) -> bool {
    let set = bitmap.count_set() as u64;
    if set <= MOST_SET_ALWAYS_SUITED {
        return true;
    }
    if set > MOST_SET {
        return false;
    }
    let occupied = patterns.occupied(FINEST_NUMBERED_LEVEL) as f64;
    let density = set as f64 / CELLS as f64;
    let cells_a_tile = cells_in_tile(FINEST_NUMBERED_LEVEL) as i32;
    let occupied_by_random_cells = tiles_in_level(FINEST_NUMBERED_LEVEL) as f64 * (1.0 - (1.0 - density).powi(cells_a_tile));
    let distinct = patterns.distinct(FINEST_NUMBERED_LEVEL) as f64;
    occupied * LEAST_CLUSTERING <= occupied_by_random_cells && distinct >= occupied * LEAST_DISTINCT_A_TILE
}

/// The bits `bitmap`'s count split takes.
pub fn bits(bitmap: &Bitmap) -> u64 {
    let set = bitmap.count_set() as u64;
    let mut bits = gamma_bits(set + 1);
    walk(bitmap, 0, CELLS, set, &mut |first_half_set, fewest, counts| bits += truncated_binary_bits(first_half_set - fewest, counts));
    bits
}

/// Writes `bitmap`'s count split.
pub fn write(bitmap: &Bitmap, stream: &mut BitStream) {
    let set = bitmap.count_set() as u64;
    stream.push_gamma(set + 1);
    walk(bitmap, 0, CELLS, set, &mut |first_half_set, fewest, counts| stream.push_truncated_binary(first_half_set - fewest, counts));
}

/// Reads a count split, setting its set cells in `cell_values`, which
/// start clear.
pub fn read(reader: &mut BitReader, cell_values: &mut Bitmap) {
    let set = reader.gamma() - 1;
    read_run(reader, cell_values, 0, CELLS, set);
}

/// Reads the run from `first`, `cells` long, `set` of them set: sets
/// them all if every one is, reads nothing if none is, else reads how
/// many lie in its first half and each half in turn.
fn read_run(reader: &mut BitReader, cell_values: &mut Bitmap, first: usize, cells: usize, set: u64) {
    if set == 0 {
        return;
    }
    if set == cells as u64 {
        cell_values.set_morton_block(first, cells);
        return;
    }
    let half = cells / 2;
    let (fewest, counts) = first_half_counts(cells, set);
    let first_half_set = fewest + reader.truncated_binary(counts);
    read_run(reader, cell_values, first, half, first_half_set);
    read_run(reader, cell_values, first + half, half, set - first_half_set);
}
