//! A run of bits: written in order, read back in the same order.
//! Multi-bit values go least significant bit first. Packed 64 to a
//! word, bit `i` of the run bit `i % 64` of word `i / 64`.

/// Bits a word holds.
const WORD_BITS: usize = u64::BITS as usize;

/// A written stream: every bit, in the order written. Clearing keeps
/// its words allocated, so one stream can be written again and again.
#[derive(Default, Clone, PartialEq, Eq, Debug)]
pub struct BitStream {
    /// The bits, packed; past the last one written, every bit is 0.
    words: Vec<u64>,
    /// How many bits have been written.
    len: usize,
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

    /// Forgets every bit written, keeping the room they took.
    pub fn clear(&mut self) {
        self.words.clear();
        self.len = 0;
    }

    /// Writes one bit.
    pub fn push(&mut self, bit: bool) {
        if self.len % WORD_BITS == 0 {
            self.words.push(0);
        }
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
