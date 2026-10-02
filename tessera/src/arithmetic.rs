//! A range coder of single bits, each at the probability given:
//! `docs/tessera.md`, "The last pass", the coder.
//!
//! Function by function: `docs/reference.md`, "`arithmetic.rs`".

use crate::bit_stream::{BitReader, BitStream, Sink};

/// The width the interval is kept at or over: a byte under the window's.
const TOP: u32 = 1 << 24;
/// Bits a byte takes, in the stream.
const BYTE_BITS: u8 = u8::BITS as u8;
/// The bits of the window the interval's ends are held in.
const WINDOW_BITS: u32 = u32::BITS;
/// A byte a carry turns to `0x00`, carrying on.
const ALL_ONES: u8 = u8::MAX;

/// Every byte with its bits in the other order: bytes go to the stream
/// highest bit first, so that the stream can end partway through one.
const REVERSED: [u8; 1 << u8::BITS] = {
    let mut reversed = [0; 1 << u8::BITS];
    let mut byte = 0;
    while byte < reversed.len() {
        reversed[byte] = (byte as u8).reverse_bits();
        byte += 1;
    }
    reversed
};

/// The probability that a bit is clear, as a fraction of `2^32`: neither
/// it nor its complement is ever under `2^21`, a 2048th -- what the
/// caller's odds never go past -- so neither part of a split rounds away.
#[derive(Clone, Copy, Debug)]
pub struct ClearProbability(pub u32);

impl ClearProbability {
    /// Where `range` splits: the width of the clear part.
    #[inline]
    fn split(self, range: u32) -> u32 {
        ((range as u64 * self.0 as u64) >> WINDOW_BITS) as u32
    }
}

/// Bits [`Encoder::finish`] takes beyond what the bits coded carry: the
/// final interval always holds an aligned run of numbers half its
/// width's power of two wide, named by at most two bits more than its
/// width's `-log2`.
pub const FINISHING_BITS: usize = 2;

/// Codes bits into a stream.
pub struct Encoder {
    /// The interval's lower end, in the window, and a carry above it.
    low: u64,
    /// The interval's width.
    range: u32,
    /// The last byte settled, held back for a carry, if any is.
    held: Option<u8>,
    /// `0xFF` bytes held back after it.
    held_all_ones: u64,
}

impl Default for Encoder {
    /// The whole interval, nothing coded.
    fn default() -> Self {
        Self { low: 0, range: u32::MAX, held: None, held_all_ones: 0 }
    }
}

impl Encoder {
    /// Codes `bit` at `clear`, the probability it is clear.
    #[inline]
    pub fn encode(&mut self, bit: bool, clear: ClearProbability, stream: &mut BitStream) {
        let split = clear.split(self.range);
        if bit {
            self.low += split as u64;
            self.range -= split;
        } else {
            self.range = split;
        }
        while self.range < TOP {
            self.settle_top_byte(stream);
            self.range <<= BYTE_BITS;
        }
    }

    /// Settles the window's top byte, and moves the window a byte on:
    /// held back if a carry could still change it, else written with
    /// every byte held before it, the carry added to them.
    fn settle_top_byte(&mut self, stream: &mut BitStream) {
        let top = (self.low >> (WINDOW_BITS - BYTE_BITS as u32)) as u8;
        let carry = self.low >> WINDOW_BITS;
        if top != ALL_ONES || carry != 0 {
            self.write_held(carry as u8, stream);
            self.held = Some(top);
        } else {
            self.held_all_ones += 1;
        }
        self.low = (self.low << BYTE_BITS) & (u32::MAX as u64);
    }

    /// Writes the bytes held back, `carry` added to them.
    fn write_held(&mut self, carry: u8, stream: &mut BitStream) {
        if let Some(held) = self.held {
            push_byte(held.wrapping_add(carry), stream);
        }
        for _ in 0..self.held_all_ones {
            push_byte(ALL_ONES.wrapping_add(carry), stream);
        }
        self.held_all_ones = 0;
    }

    /// Ends the stream: the fewest bits that keep every number they
    /// start inside the final interval, whatever bits follow them -- the
    /// bytes held back first, a carry into them if it takes one, then the
    /// window's bits down to the last one not free. So the stream ends
    /// itself: what follows it, zeros or another stream, decodes the
    /// same.
    pub fn finish(mut self, stream: &mut BitStream) {
        let end = self.low + self.range as u64;
        let (pinned, free_bits) = (0..=WINDOW_BITS)
            .rev()
            .map(|free_bits| {
                let unit = (1u64 << free_bits) - 1;
                ((self.low + unit) & !unit, free_bits)
            })
            .find(|&(number, free_bits)| number + (1u64 << free_bits) <= end)
            .expect("the interval holds at least one whole unit of its last bit");
        self.write_held((pinned >> WINDOW_BITS) as u8, stream);
        let window = pinned as u32;
        let significant = WINDOW_BITS - free_bits;
        for bit in 0..significant {
            stream.push(window >> (WINDOW_BITS - 1 - bit) & 1 == 1);
        }
    }
}

/// Writes `byte`, highest bit first.
#[inline]
fn push_byte(byte: u8, stream: &mut BitStream) {
    stream.push_value(REVERSED[byte as usize] as u64, BYTE_BITS);
}

/// Reads a byte [`push_byte`] wrote.
#[inline]
fn read_byte(reader: &mut BitReader) -> u8 {
    REVERSED[reader.value(BYTE_BITS) as usize]
}

/// Reads back bits an [`Encoder`] coded.
pub struct Decoder {
    /// The interval's width.
    range: u32,
    /// The number the stream spells, less the interval's lower end, in
    /// the window.
    offset: u32,
}

impl Decoder {
    /// Starts reading at `reader`'s next bit.
    pub fn new(reader: &mut BitReader) -> Self {
        let mut offset = 0;
        for _ in 0..WINDOW_BITS / BYTE_BITS as u32 {
            offset = offset << BYTE_BITS | read_byte(reader) as u32;
        }
        Self { range: u32::MAX, offset }
    }

    /// Reads a bit coded at `clear`.
    #[inline]
    pub fn decode(&mut self, clear: ClearProbability, reader: &mut BitReader) -> bool {
        let split = clear.split(self.range);
        let bit = self.offset >= split;
        if bit {
            self.offset -= split;
            self.range -= split;
        } else {
            self.range = split;
        }
        while self.range < TOP {
            self.range <<= BYTE_BITS;
            self.offset = self.offset << BYTE_BITS | read_byte(reader) as u32;
        }
        bit
    }
}
