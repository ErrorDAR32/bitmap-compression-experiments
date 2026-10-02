//! Range coding of single bits, each at the probability the caller gives
//! it: a bit the probability expects costs well under one bit, a surprise
//! more.
//!
//! The coder keeps an interval of the numbers in `[0, 1)`: its lower end
//! `low`, and its width `range`, a 32-bit window of a number whose bits
//! above the window are already written. A bit splits the interval at
//! its probability, clear below the split and set above it, and keeps
//! the part it is: one multiply. Whenever the width falls under `2^24`,
//! the top byte of the window is settled but for a carry, and the window
//! moves a byte on: the width is never under `2^24`, so a part is never
//! under `2^13` of it and never rounds away. A byte that a
//! later carry could still change is held back -- the last one settled,
//! and any `0xFF` bytes after it, which a carry turns to `0x00` -- and
//! written once the next byte shows whether it carries.
//!
//! The stream is the bits of one number inside the final interval,
//! every byte's highest bit first: enough of them that any bits after
//! them -- the reader reads 0 past the end -- stay inside it. Decoding
//! replays every split with the same probabilities, and reads each bit
//! off which part that number is in.

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
/// fewest that pin one number inside the final interval are at most two
/// more than its width's `-log2`.
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

    /// Ends the stream: the number in the final interval with the most
    /// clear bits at its end -- the bytes held back first, a carry into
    /// them if it takes one, then the window's bits down to its last set
    /// one, the rest read as 0.
    pub fn finish(mut self, stream: &mut BitStream) {
        let end = self.low + self.range as u64;
        let pinned = (0..=WINDOW_BITS)
            .rev()
            .map(|clear_bits| {
                let unit = (1u64 << clear_bits) - 1;
                (self.low + unit) & !unit
            })
            .find(|&number| number < end)
            .expect("a number with no clear bits at its end is the interval's own lower end");
        self.write_held((pinned >> WINDOW_BITS) as u8, stream);
        let window = pinned as u32;
        let significant = WINDOW_BITS - window.trailing_zeros().min(WINDOW_BITS);
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
