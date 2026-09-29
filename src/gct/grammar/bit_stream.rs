//! A run of bits: written in order, read back in the same order.
//! Multi-bit values go least significant bit first. Packed 64 to a
//! word, bit `i` of the run bit `i % 64` of word `i / 64`. Besides plain
//! values, the variable-length codes the grammar uses: unary, Elias
//! gamma and truncated binary.

use super::{CHILD_MASK_WIDTH, CODE_WIDTH, DIRECTION_WIDTH, FAR_WIDTH, LEAF_WIDTH, MASK_PRESENT_WIDTH, START_LEVEL_WIDTH, STREAM_MODE_WIDTH};
use crate::gct::tile::{tiles_down_to, CELLS, CELL_LEVEL, FLOOR_LEVEL};

/// Bits a word holds.
const WORD_BITS: usize = u64::BITS as usize;

/// The most bits one node takes, its values aside: a mask bit for each
/// complex tile it is nested in -- each of its own resolution, so at
/// most one a level -- and the longest header, a masking copy's: leaf,
/// code, far, direction, mask-present and a mask bit a child.
const MOST_NODE_BITS: usize = CELL_LEVEL as usize
    + (LEAF_WIDTH + CODE_WIDTH + FAR_WIDTH + DIRECTION_WIDTH + MASK_PRESENT_WIDTH) as usize
    + CHILD_MASK_WIDTH as usize;

/// The most bits a stream takes: its mode, the start level, a node at
/// every tile down to the 2x2 floor, and each cell's value said at most
/// once -- in a payload, a cell list (only ever chosen when cheaper than
/// a bit a cell) or the residual pass. A stream that is a count split
/// is only ever shorter than the tree.
pub const MOST_BITS: usize =
    STREAM_MODE_WIDTH as usize + START_LEVEL_WIDTH as usize + tiles_down_to(FLOOR_LEVEL) * MOST_NODE_BITS + CELLS;

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
        Self { words: Box::new([0; MOST_WORDS]), len: 0 }
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
    #[inline]
    pub fn push(&mut self, bit: bool) {
        self.push_value(bit as u64, 1);
    }

    /// Writes the low `width` bits of `value`, at most a word: into the
    /// word the stream ends in, and the next when they straddle it.
    #[inline]
    pub fn push_value(&mut self, value: u64, width: u8) {
        let width = width as usize;
        assert!(self.len + width <= MOST_BITS, "a stream longer than any gct writes: MOST_BITS is wrong");
        if width == 0 {
            return;
        }
        let value = if width == WORD_BITS { value } else { value & ((1 << width) - 1) };
        let (word, shift) = (self.len / WORD_BITS, self.len % WORD_BITS);
        self.words[word] |= value << shift;
        if shift + width > WORD_BITS {
            self.words[word + 1] |= value >> (WORD_BITS - shift);
        }
        self.len += width;
    }

    /// Writes `count` in unary: that many ones, then a zero.
    pub fn push_unary(&mut self, count: u64) {
        for _ in 0..count {
            self.push(true);
        }
        self.push(false);
    }

    /// Writes `value`, at least 1, in Elias gamma code: its length less
    /// one in unary, then all but its top bit.
    pub fn push_gamma(&mut self, value: u64) {
        let length = value.ilog2() as u8;
        self.push_unary(length as u64);
        self.push_value(value, length);
    }

    /// Writes `value`, one of `range` values each as likely, in truncated
    /// binary: the first values one bit shorter than the rest, so the
    /// code wastes less than a bit. Nothing at all when `range` is one.
    pub fn push_truncated_binary(&mut self, value: u64, range: u64) {
        let Some((short_width, short_codes)) = truncated_binary_shape(range) else { return };
        if value < short_codes {
            self.push_value(value, short_width);
        } else {
            // A long code: its first `short_width` bits are past every
            // short code, then one bit more.
            let long = value + short_codes;
            self.push_value(long >> 1, short_width);
            self.push_value(long & 1, 1);
        }
    }

    /// Reads from the start.
    pub fn reader(&self) -> BitReader<'_> {
        BitReader { stream: self, position: 0 }
    }
}

/// The bits of `value` in Elias gamma code, `value` at least 1.
pub const fn gamma_bits(value: u64) -> u64 {
    2 * value.ilog2() as u64 + 1
}

/// The bits of `value`, one of `range`, in truncated binary.
pub const fn truncated_binary_bits(value: u64, range: u64) -> u64 {
    match truncated_binary_shape(range) {
        None => 0,
        Some((short_width, short_codes)) => short_width as u64 + (value >= short_codes) as u64,
    }
}

/// A truncated binary code of `range` values: the short codes' width,
/// and how many values take a short code; `None` for one value, which
/// needs no bits.
const fn truncated_binary_shape(range: u64) -> Option<(u8, u64)> {
    if range <= 1 {
        return None;
    }
    let short_width = range.ilog2() as u8;
    Some((short_width, (1 << (short_width + 1)) - range))
}

/// Reads a [`BitStream`] back, in order. Past the end, bits read as 0.
pub struct BitReader<'a> {
    /// The stream read from.
    stream: &'a BitStream,
    /// The next bit to read.
    position: usize,
}

impl BitReader<'_> {
    /// Reads one bit.
    #[inline]
    pub fn bit(&mut self) -> bool {
        self.value(1) == 1
    }

    /// Reads what [`BitStream::push_unary`] wrote.
    pub fn unary(&mut self) -> u64 {
        let mut count = 0;
        while self.bit() {
            count += 1;
        }
        count
    }

    /// Reads what [`BitStream::push_gamma`] wrote.
    pub fn gamma(&mut self) -> u64 {
        let length = self.unary() as u8;
        1 << length | self.value(length)
    }

    /// Reads what [`BitStream::push_truncated_binary`] wrote, of `range`.
    pub fn truncated_binary(&mut self, range: u64) -> u64 {
        let Some((short_width, short_codes)) = truncated_binary_shape(range) else { return 0 };
        let first = self.value(short_width);
        if first < short_codes {
            return first;
        }
        (first << 1 | self.value(1)) - short_codes
    }

    /// Reads `width` bits, at most a word, as written by
    /// [`BitStream::push_value`]: from the word the next bit is in, and
    /// the next when they straddle it. Past the stream's end every bit is
    /// 0, and so is every word past the most a stream takes.
    #[inline]
    pub fn value(&mut self, width: u8) -> u64 {
        let width = width as usize;
        if width == 0 {
            return 0;
        }
        let (word, shift) = (self.position / WORD_BITS, self.position % WORD_BITS);
        let words = &self.stream.words;
        let mut value = words.get(word).map_or(0, |&low| low >> shift);
        if shift + width > WORD_BITS {
            value |= words.get(word + 1).map_or(0, |&high| high << (WORD_BITS - shift));
        }
        self.position += width;
        if width == WORD_BITS {
            value
        } else {
            value & ((1 << width) - 1)
        }
    }
}
