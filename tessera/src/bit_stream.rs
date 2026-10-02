//! A run of bits: written in order, read back in the same order.
//! Multi-bit values go least significant bit first. Packed 64 to a
//! word, bit `i` of the run bit `i % 64` of word `i / 64`. Besides plain
//! values, the variable-length codes the grammar uses: unary, Elias
//! gamma and truncated binary -- written to a [`Sink`]: the stream, or a
//! [`Counter`] of the bits they would take.

use crate::quadtree_writer::MOST_NODE_BITS;
use crate::last_pass::MOST_EXTRA_BITS;
use crate::tile::{tiles_down_to, CELLS, FLOOR_LEVEL};

/// Bits a word holds.
const WORD_BITS: usize = u64::BITS as usize;
/// Bits a byte holds.
const BYTE_BITS: usize = u8::BITS as usize;
/// Bytes a word holds.
const WORD_BYTES: usize = WORD_BITS / BYTE_BITS;

/// The most bits a stream takes (`docs/tessera.md`, "Memory").
pub const MOST_BITS: usize = tiles_down_to(FLOOR_LEVEL) * MOST_NODE_BITS + CELLS + MOST_EXTRA_BITS;

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
    /// size to the word, for keeping it in memory once written. The bits
    /// past the last one written are 0.
    pub fn words(&self) -> &[u64] {
        &self.words[..self.len.div_ceil(WORD_BITS)]
    }

    /// Makes the stream the bits in `words`, whatever it held before:
    /// what [`BitStream::words`] gave, read back. Its length is every bit
    /// of every word -- the last word's bits past the stream written are
    /// read as the 0s they are -- so a stream loaded is for reading: bits
    /// pushed after it would follow that padding.
    pub fn load_words(&mut self, words: &[u64]) {
        assert!(words.len() <= MOST_WORDS, "a stream longer than any Tessera writes");
        self.clear();
        self.words[..words.len()].copy_from_slice(words);
        self.len = words.len() * WORD_BITS;
    }

    /// The bits written, packed into bytes, the first bit lowest in the
    /// first byte, and no more bytes than they take: the stream at its
    /// size to the byte, for keeping it on disk. The bits past the
    /// last one written are 0.
    pub fn to_bytes(&self) -> Box<[u8]> {
        let mut bytes = vec![0u8; self.len.div_ceil(BYTE_BITS)].into_boxed_slice();
        for (chunk, word) in bytes.chunks_mut(WORD_BYTES).zip(self.words.iter()) {
            chunk.copy_from_slice(&word.to_le_bytes()[..chunk.len()]);
        }
        bytes
    }

    /// Makes the stream the bits in `bytes`, whatever it held before:
    /// what [`BitStream::to_bytes`] gave, read back. Its length is every
    /// bit of every byte -- the last byte's bits past the stream written
    /// are read as the 0s they are -- so a stream loaded is for reading:
    /// bits pushed after it would follow that padding.
    pub fn load_bytes(&mut self, bytes: &[u8]) {
        assert!(bytes.len() <= MOST_WORDS * WORD_BYTES, "a stream longer than any Tessera writes");
        self.clear();
        for (word, chunk) in self.words.iter_mut().zip(bytes.chunks(WORD_BYTES)) {
            let mut word_bytes = [0u8; WORD_BYTES];
            word_bytes[..chunk.len()].copy_from_slice(chunk);
            *word = u64::from_le_bytes(word_bytes);
        }
        self.len = bytes.len() * BYTE_BITS;
    }

    /// Reads from the start.
    pub fn reader(&self) -> BitReader<'_> {
        BitReader { stream: self, next_word: 0, buffer: 0, buffered: 0 }
    }
}

/// Where written bits go: a [`BitStream`], or a [`Counter`] of them.
pub trait Sink {
    /// Writes the low `width` bits of `value`, at most a word: into the
    /// word the stream ends in, and the next when they straddle it.
    fn push_value(&mut self, value: u64, width: u8);

    /// The count of bits written, if this only counts them: what is
    /// written can then be counted without looking at it.
    #[inline]
    fn counted(&mut self) -> Option<&mut u64> {
        None
    }

    /// Writes one bit.
    #[inline]
    fn push(&mut self, bit: bool) {
        self.push_value(bit as u64, 1);
    }

    /// Writes `count` in unary: that many ones, then a zero.
    fn push_unary(&mut self, count: u64) {
        if let Some(bits) = self.counted() {
            *bits += count + 1;
            return;
        }
        for _ in 0..count {
            self.push(true);
        }
        self.push(false);
    }

    /// Writes `value`, at least 1, in Elias gamma code: its length less
    /// one in unary, then all but its top bit.
    fn push_gamma(&mut self, value: u64) {
        let length = value.ilog2() as u8;
        self.push_unary(length as u64);
        self.push_value(value, length);
    }

    /// Writes `value`, one of `range` values each as likely, in truncated
    /// binary: the first values one bit shorter than the rest, so the
    /// code wastes less than a bit. Nothing at all when `range` is one.
    fn push_truncated_binary(&mut self, value: u64, range: u64) {
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
}

impl Sink for BitStream {
    #[inline]
    fn push_value(&mut self, value: u64, width: u8) {
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
}

/// Counts the bits written, keeping none.
#[derive(Default)]
pub struct Counter(pub u64);

impl Sink for Counter {
    #[inline]
    fn push_value(&mut self, _: u64, width: u8) {
        self.0 += width as u64;
    }

    #[inline]
    fn counted(&mut self) -> Option<&mut u64> {
        Some(&mut self.0)
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
