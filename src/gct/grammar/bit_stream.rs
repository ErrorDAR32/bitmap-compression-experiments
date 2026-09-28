//! A run of bits: written in order, read back in the same order.
//! Multi-bit values go least significant bit first. Packed 64 to a
//! word, bit `i` of the run bit `i % 64` of word `i / 64`.

use super::{CODE_WIDTH, DIRECTION_WIDTH, FAR_WIDTH, LEAF_WIDTH, MASK_BIT_WIDTH, MASK_PRESENT_WIDTH, START_LEVEL_WIDTH};
use crate::gct::tile::{tiles_down_to, CELLS, CELL_LEVEL, CHILDREN_ACROSS};

/// Bits a word holds.
const WORD_BITS: usize = u64::BITS as usize;

/// The most bits one node takes, its values aside: a mask bit for each
/// complex tile it is nested in -- each of its own resolution, so at
/// most one a level -- and the longest header, a masking copy's: leaf,
/// code, far, direction, mask-present and a mask bit a child.
const MOST_NODE_BITS: usize = CELL_LEVEL as usize
    + (LEAF_WIDTH + CODE_WIDTH + FAR_WIDTH + DIRECTION_WIDTH + MASK_PRESENT_WIDTH) as usize
    + (CHILDREN_ACROSS * CHILDREN_ACROSS * MASK_BIT_WIDTH) as usize;

/// The most bits a stream takes: the start level, a node at every tile
/// down to the 2x2 floor, and each cell's value said at most once -- in
/// a payload, a point list (only ever chosen when cheaper than a bit a
/// cell) or the residual pass.
pub const MOST_BITS: usize = START_LEVEL_WIDTH as usize + tiles_down_to(CELL_LEVEL - 1) * MOST_NODE_BITS + CELLS;

/// Words the most bits a stream takes fill.
const MOST_WORDS: usize = MOST_BITS.div_ceil(WORD_BITS);

/// A written stream: every bit, in the order written. Its words are
/// allocated once, as many as any stream can take, and never grow.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BitStream {
    /// The bits, packed; past the last one written, every bit is 0.
    words: Box<[u64; MOST_WORDS]>,
    /// How many bits have been written.
    len: usize,
}

impl Default for BitStream {
    /// An empty stream, all its room allocated.
    fn default() -> Self {
        let words: Box<[u64]> = std::iter::repeat_n(0, MOST_WORDS).collect();
        Self { words: words.try_into().unwrap_or_else(|_| unreachable!("exactly MOST_WORDS words")), len: 0 }
    }
}

impl BitStream {
    /// How many bits have been written.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether nothing has been written.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Forgets every bit written: the words they took are zero again.
    pub fn clear(&mut self) {
        self.words[..self.len.div_ceil(WORD_BITS)].fill(0);
        self.len = 0;
    }

    /// Writes one bit.
    pub fn push(&mut self, bit: bool) {
        assert!(self.len < MOST_BITS, "a stream longer than any gct writes: MOST_BITS is wrong");
        if bit {
            self.words[self.len / WORD_BITS] |= 1 << (self.len % WORD_BITS);
        }
        self.len += 1;
    }

    /// Writes the low `width` bits of `value`.
    pub fn push_value(&mut self, value: u64, width: u8) {
        for bit in 0..width {
            self.push(value >> bit & 1 == 1);
        }
    }

    /// Reads from the start.
    pub fn reader(&self) -> BitReader<'_> {
        BitReader { stream: self, at: 0 }
    }
}

/// Reads a [`BitStream`] back, in order. Past the end, bits read as 0.
pub struct BitReader<'a> {
    /// The stream read from.
    stream: &'a BitStream,
    /// The next bit to read.
    at: usize,
}

impl BitReader<'_> {
    /// Reads one bit.
    pub fn bit(&mut self) -> bool {
        let bit = self.at < self.stream.len && self.stream.words[self.at / WORD_BITS] >> (self.at % WORD_BITS) & 1 == 1;
        self.at += 1;
        bit
    }

    /// Reads `width` bits, as written by [`BitStream::push_value`].
    pub fn value(&mut self, width: u8) -> u64 {
        (0..width).fold(0, |value, bit| value | (self.bit() as u64) << bit)
    }
}
