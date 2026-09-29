//! The count split: what the stream says instead of the tree when it
//! takes fewer bits -- mostly sparse cells, with no whole areas to bind
//! and nothing to copy, the tree's worst case.
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
use crate::gct::tile::CELLS;
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
/// and how many counts could. Stops as soon as `say` returns false, and
/// returns false then.
fn walk(bitmap: &Bitmap, first: usize, cells: usize, set: u64, say: &mut impl FnMut(u64, u64, u64) -> bool) -> bool {
    if set == 0 || set == cells as u64 {
        return true;
    }
    let half = cells / 2;
    let first_half_set = bitmap.count_in_morton_block(first, half);
    let (fewest, counts) = first_half_counts(cells, set);
    say(first_half_set, fewest, counts)
        && walk(bitmap, first, half, first_half_set, say)
        && walk(bitmap, first + half, half, set - first_half_set, say)
}

/// The most cells a bitmap can have set for its count split to be tried
/// at all: an eighth. The count split is the fallback for sparse cells,
/// and counting it walks every run holding some set cells and some clear
/// -- on a denser bitmap nearly every run, on every bitmap encoded, for
/// the few it would win on.
const MOST_SET_TRIED: u64 = (CELLS / 8) as u64;

/// The bits `bitmap`'s count split takes, however many cells are set.
pub fn bits(bitmap: &Bitmap) -> u64 {
    counted_below(bitmap, bitmap.count_set() as u64, u64::MAX).expect("a count split takes fewer bits than any count")
}

/// The bits `bitmap`'s count split takes, if it is tried at all -- at
/// most an eighth of the cells set -- and they are fewer than `limit`.
pub fn bits_below(bitmap: &Bitmap, limit: u64) -> Option<u64> {
    let set = bitmap.count_set() as u64;
    (set <= MOST_SET_TRIED).then(|| counted_below(bitmap, set, limit)).flatten()
}

/// The bits the count split of `bitmap`, `set` cells of it set, takes,
/// if fewer than `limit`: counted no further once they are not.
fn counted_below(bitmap: &Bitmap, set: u64, limit: u64) -> Option<u64> {
    let mut bits = gamma_bits(set + 1);
    let under = bits < limit
        && walk(bitmap, 0, CELLS, set, &mut |first_half_set, fewest, counts| {
            bits += truncated_binary_bits(first_half_set - fewest, counts);
            bits < limit
        });
    under.then_some(bits)
}

/// Writes `bitmap`'s count split.
pub fn write(bitmap: &Bitmap, stream: &mut BitStream) {
    let set = bitmap.count_set() as u64;
    stream.push_gamma(set + 1);
    walk(bitmap, 0, CELLS, set, &mut |first_half_set, fewest, counts| {
        stream.push_truncated_binary(first_half_set - fewest, counts);
        true
    });
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
