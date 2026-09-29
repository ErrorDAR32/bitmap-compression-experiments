//! The count split: what the stream says instead of the tree for a
//! bitmap whose count split takes fewer bits than its tree would --
//! sparse cells, clustered, with nothing to copy: the tree's worst case.
//! Only one of the two is ever made: which is judged from the greedy
//! tiler's tiles, before the complex tiler
//! ([`Gct::encode`](crate::gct::Gct::encode)).
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
const fn first_half_counts(cells: usize, set: u64) -> (u64, u64) {
    let half = (cells / 2) as u64;
    let fewest = set.saturating_sub(half);
    let most = if set < half { set } else { half };
    (fewest, most - fewest + 1)
}

/// The bits a run of the cells `run` holds, `cells` of them, `set` of
/// them set, takes inside it: what it says of its halves, and they of
/// theirs.
const fn run_bits(run: u64, cells: usize, set: u64) -> u64 {
    if set == 0 || set == cells as u64 {
        return 0;
    }
    let half = cells / 2;
    let first_half = run & ((1 << half) - 1);
    let first_half_set = first_half.count_ones() as u64;
    let (fewest, counts) = first_half_counts(cells, set);
    truncated_binary_bits(first_half_set - fewest, counts)
        + run_bits(first_half, half, first_half_set)
        + run_bits(run >> half, half, set - first_half_set)
}

/// Runs this short are counted by looking them up in [`BYTE_BITS`]: a
/// byte of cells.
const BYTE_CELLS: usize = u8::BITS as usize;

/// The bits every run of a byte of cells takes inside it, by its cells.
const BYTE_BITS: [u8; 1 << BYTE_CELLS] = {
    let mut bits = [0; 1 << BYTE_CELLS];
    let mut run = 0;
    while run < bits.len() {
        bits[run] = run_bits(run as u64, BYTE_CELLS, run.count_ones() as u64) as u8;
        run += 1;
    }
    bits
};

/// Counting a count split's bits, up to a limit.
struct Counter<'a> {
    /// The bitmap counted.
    bitmap: &'a Bitmap,
    /// The bits counted so far.
    bits: u64,
    /// Where counting stops.
    limit: u64,
}

impl Counter<'_> {
    /// Counts the run from `first`, `cells` long, `set` of them set,
    /// unless the bits reach the limit first: whether they stayed under
    /// it.
    fn run(&mut self, first: usize, cells: usize, set: u64) -> bool {
        if set == 0 || set == cells as u64 {
            return true;
        }
        if cells == BYTE_CELLS {
            self.bits += BYTE_BITS[self.bitmap.morton_run(first, cells) as usize] as u64;
            return self.bits < self.limit;
        }
        let half = cells / 2;
        let first_half_set = self.bitmap.count_in_morton_block(first, half);
        let (fewest, counts) = first_half_counts(cells, set);
        self.bits += truncated_binary_bits(first_half_set - fewest, counts);
        self.bits < self.limit && self.run(first, half, first_half_set) && self.run(first + half, half, set - first_half_set)
    }
}

/// The bits `bitmap`'s count split takes, if fewer than `limit`:
/// counting stops as soon as they reach it.
pub fn bits_under(bitmap: &Bitmap, limit: u64) -> Option<u64> {
    let set = bitmap.count_set() as u64;
    let mut counter = Counter { bitmap, bits: gamma_bits(set + 1), limit };
    (counter.run(0, CELLS, set) && counter.bits < limit).then_some(counter.bits)
}

/// The bits `bitmap`'s count split takes.
pub fn bits(bitmap: &Bitmap) -> u64 {
    bits_under(bitmap, u64::MAX).expect("no count split takes every bit there is")
}

/// Writes `bitmap`'s count split.
pub fn write(bitmap: &Bitmap, stream: &mut BitStream) {
    let set = bitmap.count_set() as u64;
    stream.push_gamma(set + 1);
    write_run(bitmap, stream, 0, CELLS, set);
}

/// Writes the run from `first`, `cells` long, `set` of them set: nothing
/// if all or none is, else how many lie in its first half and each half
/// in turn.
fn write_run(bitmap: &Bitmap, stream: &mut BitStream, first: usize, cells: usize, set: u64) {
    if set == 0 || set == cells as u64 {
        return;
    }
    let half = cells / 2;
    let first_half_set = bitmap.count_in_morton_block(first, half);
    let (fewest, counts) = first_half_counts(cells, set);
    stream.push_truncated_binary(first_half_set - fewest, counts);
    write_run(bitmap, stream, first, half, first_half_set);
    write_run(bitmap, stream, first + half, half, set - first_half_set);
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
