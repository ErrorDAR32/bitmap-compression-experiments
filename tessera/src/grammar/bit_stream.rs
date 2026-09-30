//! A run of bits: written in order, read back in the same order.
//! Multi-bit values go least significant bit first. Packed 64 to a
//! word, bit `i` of the run bit `i % 64` of word `i / 64`. Besides plain
//! values, the variable-length codes the grammar uses: unary, Elias
//! gamma and truncated binary.

use super::{CHILD_MASK_WIDTH, CODE_WIDTH, DIRECTION_WIDTH, FAR_WIDTH, LEAF_WIDTH, MASK_PRESENT_WIDTH, START_LEVEL_WIDTH, STREAM_MODE_WIDTH};
use crate::last_pass::MOST_EXTRA_BITS;
use crate::tile::{tiles_down_to, CELLS, FLOOR_LEVEL};

/// Bits a word holds.
const WORD_BITS: usize = u64::BITS as usize;

/// The most bits one node takes, its values aside: the longest header,
/// a masking copy's -- leaf, code, far, direction, mask-present and a
/// mask bit a child.
const MOST_NODE_BITS: usize = (LEAF_WIDTH + CODE_WIDTH + FAR_WIDTH + DIRECTION_WIDTH + MASK_PRESENT_WIDTH) as usize + CHILD_MASK_WIDTH as usize;

/// The most bits a stream takes: its mode, the start level, a node at
/// every tile down to the 4x4 floor, and each cell's value said at most
/// once -- in a payload, a cell list (only ever chosen when cheaper than
/// a bit a cell) or the last pass, which may take a little more than a
/// bit a cell. A stream that is a count split is only ever shorter than
/// the tree.
pub const MOST_BITS: usize = STREAM_MODE_WIDTH as usize
    + START_LEVEL_WIDTH as usize
    + tiles_down_to(FLOOR_LEVEL) * MOST_NODE_BITS
    + CELLS
    + MOST_EXTRA_BITS;

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

    /// The words the bits written take, and no more: the stream at its
    /// exact size, for keeping it once written. The bits past the last
    /// one written are 0.
    pub fn words(&self) -> &[u64] {
        &self.words[..self.len.div_ceil(WORD_BITS)]
    }

    /// Makes the stream `len` bits held in `words`, whatever it held
    /// before: what [`BitStream::words`] gave, read back. `words` must
    /// be exactly the words `len` bits take, their bits past `len` 0;
    /// anything else is a bug, and panics.
    pub fn load(&mut self, words: &[u64], len: usize) {
        assert!(len <= MOST_BITS, "a stream longer than any Tessera writes");
        assert_eq!(words.len(), len.div_ceil(WORD_BITS), "not the words {len} bits take");
        let used_in_last = len % WORD_BITS;
        assert!(
            used_in_last == 0 || words.last().is_none_or(|&last| last >> used_in_last == 0),
            "bits set past the stream's end"
        );
        self.clear();
        self.words[..words.len()].copy_from_slice(words);
        self.len = len;
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
        assert!(self.len + width <= MOST_BITS, "a stream longer than any Tessera writes: MOST_BITS is wrong");
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
        BitReader { stream: self, next_word: 0, buffer: 0, buffered: 0 }
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
pub(super) const fn truncated_binary_shape(range: u64) -> Option<(u8, u64)> {
    if range <= 1 {
        return None;
    }
    let short_width = range.ilog2() as u8;
    Some((short_width, (1 << (short_width + 1)) - range))
}

/// Reads a [`BitStream`] back, in order, a word at a time: each word
/// taken off the stream once, into a buffer the bits are read from.
/// Past the end, bits read as 0.
pub struct BitReader<'a> {
    /// The stream read from.
    stream: &'a BitStream,
    /// The next word to take off the stream.
    next_word: usize,
    /// The bits taken off the stream not read yet, the next one lowest;
    /// every bit above them 0.
    buffer: u64,
    /// How many bits the buffer holds.
    buffered: u32,
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

    /// The next `width` bits, at most a word, as [`BitReader::value`]
    /// would read them, left unread.
    #[inline]
    pub fn peek(&self, width: u8) -> u64 {
        let next_word = self.stream.words.get(self.next_word).copied().unwrap_or(0);
        (self.buffer | next_word.checked_shl(self.buffered).unwrap_or(0)) & low_bits(width as u32)
    }

    /// Passes over the next `width` bits, at most a word.
    #[inline]
    pub fn skip(&mut self, width: u8) {
        self.value(width);
    }

    /// Reads `width` bits, at most a word, as written by
    /// [`BitStream::push_value`]: off the buffer, and when it holds too
    /// few, all it holds and the rest off the next word, whose bits left
    /// over become the buffer. Past the stream's end every bit is 0, and
    /// so is every word past the most a stream takes.
    #[inline]
    pub fn value(&mut self, width: u8) -> u64 {
        let width = width as u32;
        if width <= self.buffered {
            let value = self.buffer & low_bits(width);
            // Shifting a whole word's width out leaves nothing.
            self.buffer = self.buffer.checked_shr(width).unwrap_or(0);
            self.buffered -= width;
            return value;
        }
        let word = self.stream.words.get(self.next_word).copied().unwrap_or(0);
        self.next_word += 1;
        let (value, taken_from_word) = (self.buffer | word << self.buffered, width - self.buffered);
        self.buffer = word.checked_shr(taken_from_word).unwrap_or(0);
        self.buffered = u64::BITS - taken_from_word;
        value & low_bits(width)
    }
}

/// A word with its low `width` bits set, `width` at most a word.
fn low_bits(width: u32) -> u64 {
    u64::MAX.checked_shr(u64::BITS - width).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stream's words, loaded into another stream, read back the same
    /// bits, whatever that stream held before.
    #[test]
    fn words_load_back_into_the_same_stream() {
        let mut written = BitStream::default();
        for value in 0..40u64 {
            written.push_value(value * 2_654_435_761 % 1000, 10);
        }
        written.push(true);
        let mut loaded = BitStream::default();
        loaded.push_value(u64::MAX, 64);
        loaded.load(written.words(), written.len());
        assert_eq!(loaded, written);
    }

    #[test]
    #[should_panic(expected = "bits set past the stream's end")]
    fn loading_bits_past_the_end_panics() {
        BitStream::default().load(&[0b100], 2);
    }
}
