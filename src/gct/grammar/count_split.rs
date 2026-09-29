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

use super::bit_stream::{gamma_bits, truncated_binary_bits, truncated_binary_shape, BitReader, BitStream};
use crate::WORDS;
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

/// A byte of cells: what [`SHORT_RUN_BITS`] is made from.
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

/// The most bits a run of a byte of cells takes inside it.
const MOST_BYTE_RUN_BITS: u8 = {
    let mut most = 0;
    let mut run = 0;
    while run < BYTE_BITS.len() {
        if BYTE_BITS[run] > most {
            most = BYTE_BITS[run];
        }
        run += 1;
    }
    most
};

/// A run of a byte of cells, as read back: its cells, and how many bits
/// saying them took.
#[derive(Clone, Copy)]
struct ByteRun {
    /// The cells, bit `i` the run's cell `i`.
    cells: u8,
    /// The bits read.
    bits: u8,
}

/// Every run of a byte of cells as read back, by how many of its cells
/// are set and the next [`MOST_BYTE_RUN_BITS`] bits of the stream: what
/// [`read_word`] would read, made when compiling by reading the same way
/// off those bits. 18 KiB.
static BYTE_RUNS: [[ByteRun; 1 << MOST_BYTE_RUN_BITS]; BYTE_CELLS + 1] = {
    let mut runs = [[ByteRun { cells: 0, bits: 0 }; 1 << MOST_BYTE_RUN_BITS]; BYTE_CELLS + 1];
    let mut set = 0;
    while set <= BYTE_CELLS {
        let mut said = 0;
        while said < 1 << MOST_BYTE_RUN_BITS {
            let (cells, bits) = read_run_off(said as u64, BYTE_CELLS, set as u64);
            runs[set][said] = ByteRun { cells: cells as u8, bits: bits as u8 };
            said += 1;
        }
        set += 1;
    }
    runs
};

/// Reads a run of `cells` cells, `set` of them set, off the bits `said`,
/// the next one lowest, as [`read_word`] reads it off the stream: its
/// cells, and how many bits it took.
const fn read_run_off(said: u64, cells: usize, set: u64) -> (u64, u32) {
    if set == 0 || set == cells as u64 {
        return (if set == 0 { 0 } else { (1 << cells) - 1 }, 0);
    }
    let half = cells / 2;
    let (fewest, counts) = first_half_counts(cells, set);
    let (count, count_bits) = read_truncated_binary_off(said, counts);
    let first_half_set = fewest + count;
    let (first_half, first_half_bits) = read_run_off(said >> count_bits, half, first_half_set);
    let (second_half, second_half_bits) = read_run_off(said >> (count_bits + first_half_bits), half, set - first_half_set);
    (first_half | second_half << half, count_bits + first_half_bits + second_half_bits)
}

/// Reads a value of `range` in truncated binary off the bits `said`, as
/// [`BitReader::truncated_binary`] reads it off the stream: the value,
/// and how many bits it took.
const fn read_truncated_binary_off(said: u64, range: u64) -> (u64, u32) {
    let Some((short_width, short_codes)) = truncated_binary_shape(range) else { return (0, 0) };
    let first = said & ((1 << short_width) - 1);
    if first < short_codes {
        return (first, short_width as u32);
    }
    ((first << 1 | (said >> short_width & 1)) - short_codes, short_width as u32 + 1)
}

/// Runs this short are counted by looking them up in [`SHORT_RUN_BITS`]:
/// two bytes of cells, a quarter of a word.
const SHORT_RUN_CELLS: usize = 2 * BYTE_CELLS;

/// The bits every run of [`SHORT_RUN_CELLS`] cells takes inside it, by
/// its cells, made when compiling: its own count, then each byte's bits.
/// 64 KiB.
static SHORT_RUN_BITS: [u8; 1 << SHORT_RUN_CELLS] = {
    let mut bits = [0; 1 << SHORT_RUN_CELLS];
    let mut run = 0;
    while run < bits.len() {
        let set = (run as u64).count_ones() as u64;
        if set != 0 && set != SHORT_RUN_CELLS as u64 {
            let (first_byte, second_byte) = (run & 0xFF, run >> BYTE_CELLS);
            let (fewest, counts) = first_half_counts(SHORT_RUN_CELLS, set);
            let own = truncated_binary_bits((first_byte as u64).count_ones() as u64 - fewest, counts);
            bits[run] = (own + BYTE_BITS[first_byte] as u64 + BYTE_BITS[second_byte] as u64) as u8;
        }
        run += 1;
    }
    bits
};

/// The bits a run of the cells `run` holds, `cells` of them -- a word
/// or less -- `set` of them set, takes inside it: as [`run_bits`], each
/// quarter of a word looked up in [`SHORT_RUN_BITS`].
fn word_bits(run: u64, cells: usize, set: u64) -> u64 {
    if set == 0 || set == cells as u64 {
        return 0;
    }
    if cells == SHORT_RUN_CELLS {
        return SHORT_RUN_BITS[run as usize] as u64;
    }
    let half = cells / 2;
    let first_half = run & ((1 << half) - 1);
    let first_half_set = first_half.count_ones() as u64;
    let (fewest, counts) = first_half_counts(cells, set);
    truncated_binary_bits(first_half_set - fewest, counts)
        + word_bits(first_half, half, first_half_set)
        + word_bits(run >> half, half, set - first_half_set)
}

/// Cells a word of the bitmap holds: a run of the Morton order.
const WORD_CELLS: usize = u64::BITS as usize;

/// A bitmap's words, and how many cells are set before each: what a
/// run longer than a word holds in each half is read off those.
struct Words<'a> {
    /// The bitmap's words, in Morton order.
    words: &'a [u64; WORDS],
    /// Cells set before each word, and in all.
    set_before: [u32; WORDS + 1],
}

impl<'a> Words<'a> {
    /// `bitmap`'s words, counted.
    fn of(bitmap: &'a Bitmap) -> Self {
        let words = bitmap.words();
        let mut set_before = [0u32; WORDS + 1];
        for (index, word) in words.iter().enumerate() {
            set_before[index + 1] = set_before[index] + word.count_ones();
        }
        Self { words, set_before }
    }

    /// Cells set in all.
    fn set(&self) -> u64 {
        self.set_before[WORDS] as u64
    }

    /// Cells set in the first half of the run of `count` words from
    /// `first`.
    fn first_half_set(&self, first: usize, count: usize) -> u64 {
        (self.set_before[first + count / 2] - self.set_before[first]) as u64
    }

    /// The bits the run of `count` words from `first`, `set` of its
    /// cells set, takes inside it.
    fn bits(&self, first: usize, count: usize, set: u64) -> u64 {
        if set == 0 || set == (count * WORD_CELLS) as u64 {
            return 0;
        }
        if count == 1 {
            return word_bits(self.words[first], WORD_CELLS, set);
        }
        let (half, first_half_set) = (count / 2, self.first_half_set(first, count));
        let (fewest, counts) = first_half_counts(count * WORD_CELLS, set);
        truncated_binary_bits(first_half_set - fewest, counts)
            + self.bits(first, half, first_half_set)
            + self.bits(first + half, half, set - first_half_set)
    }

    /// Writes the run of `count` words from `first`, `set` of its cells
    /// set: nothing if all or none is, else how many lie in its first
    /// half and each half in turn.
    fn write(&self, stream: &mut BitStream, first: usize, count: usize, set: u64) {
        if set == 0 || set == (count * WORD_CELLS) as u64 {
            return;
        }
        if count == 1 {
            write_word(stream, self.words[first], WORD_CELLS, set);
            return;
        }
        let (half, first_half_set) = (count / 2, self.first_half_set(first, count));
        let (fewest, counts) = first_half_counts(count * WORD_CELLS, set);
        stream.push_truncated_binary(first_half_set - fewest, counts);
        self.write(stream, first, half, first_half_set);
        self.write(stream, first + half, half, set - first_half_set);
    }
}

/// Writes the run of the cells `run` holds, `cells` of them -- a word or
/// less -- `set` of them set, as [`Words::write`] a longer one.
fn write_word(stream: &mut BitStream, run: u64, cells: usize, set: u64) {
    if set == 0 || set == cells as u64 {
        return;
    }
    let half = cells / 2;
    let first_half = run & ((1 << half) - 1);
    let first_half_set = first_half.count_ones() as u64;
    let (fewest, counts) = first_half_counts(cells, set);
    stream.push_truncated_binary(first_half_set - fewest, counts);
    write_word(stream, first_half, half, first_half_set);
    write_word(stream, run >> half, half, set - first_half_set);
}

/// The bits `bitmap`'s count split takes: a step a word, whatever the
/// bitmap -- a run inside one word counted off that word alone.
pub fn bits(bitmap: &Bitmap) -> u64 {
    let words = Words::of(bitmap);
    gamma_bits(words.set() + 1) + words.bits(0, WORDS, words.set())
}

/// Writes `bitmap`'s count split.
pub fn write(bitmap: &Bitmap, stream: &mut BitStream) {
    let words = Words::of(bitmap);
    stream.push_gamma(words.set() + 1);
    words.write(stream, 0, WORDS, words.set());
}

/// Reads a count split into `cell_values`, which start clear.
pub fn read(reader: &mut BitReader, cell_values: &mut Bitmap) {
    let set = reader.gamma() - 1;
    read_words(reader, cell_values.words_mut(), set);
}

/// Reads the run of `words`' words -- all of them, clear to start with
/// -- `set` of its cells set: sets every cell if every one is, reads
/// nothing if none is, else reads how many lie in its first half and
/// each half in turn.
fn read_words(reader: &mut BitReader, words: &mut [u64], set: u64) {
    let cells = words.len() * WORD_CELLS;
    if set == 0 {
        return;
    }
    if words.len() == 1 {
        words[0] = read_word(reader, WORD_CELLS, set);
        return;
    }
    if set == cells as u64 {
        words.fill(u64::MAX);
        return;
    }
    if set == 1 {
        let place = read_lone_cell_place(reader, cells);
        words[place / WORD_CELLS] = 1 << (place % WORD_CELLS);
        return;
    }
    let (fewest, counts) = first_half_counts(cells, set);
    let first_half_set = fewest + reader.truncated_binary(counts);
    let (first_half, second_half) = words.split_at_mut(words.len() / 2);
    read_words(reader, first_half, first_half_set);
    read_words(reader, second_half, set - first_half_set);
}

/// Reads the run of `cells` cells -- a word or less -- `set` of them
/// set, as [`read_words`] a longer one, and gives its cells: bit `i` the
/// run's cell `i`.
fn read_word(reader: &mut BitReader, cells: usize, set: u64) -> u64 {
    if set == 0 {
        return 0;
    }
    if set == cells as u64 {
        return u64::MAX >> (WORD_CELLS - cells);
    }
    if set == 1 {
        return 1 << read_lone_cell_place(reader, cells);
    }
    if cells == BYTE_CELLS {
        let run = BYTE_RUNS[set as usize][reader.peek(MOST_BYTE_RUN_BITS) as usize];
        reader.skip(run.bits);
        return run.cells as u64;
    }
    let half = cells / 2;
    let (fewest, counts) = first_half_counts(cells, set);
    let first_half_set = fewest + reader.truncated_binary(counts);
    let first_half = read_word(reader, half, first_half_set);
    first_half | read_word(reader, half, set - first_half_set) << half
}

/// Reads where the one set cell of a run of `cells` cells is. Each
/// halving of such a run says a bit -- whether the cell is in its first
/// half, one of two counts -- and only the half holding it is halved
/// again: so the run says the cell's place, from its top bit down, each
/// bit flipped, read here at once.
fn read_lone_cell_place(reader: &mut BitReader, cells: usize) -> usize {
    let halvings = cells.ilog2();
    let said = reader.value(halvings as u8);
    ((!said).reverse_bits() >> (u64::BITS - halvings)) as usize
}
