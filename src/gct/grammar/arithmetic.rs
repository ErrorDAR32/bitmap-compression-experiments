//! Arithmetic coding of single bits, each at odds the caller gives: a
//! bit the odds expect costs well under one bit, a surprise more.
//!
//! The coder keeps an interval of the numbers in `[0, 1)`, held as two
//! 32-bit integers, `low` and `high`, both inclusive. A
//! bit splits the interval at its odds, clear below the split and set
//! above it, and keeps the part it is. Whenever the interval lies in one
//! half, the first bit of every number in it is settled: it is written,
//! and the interval doubled. When it straddles the middle within the
//! middle half, the next bit is settled only later, and to the opposite
//! of the one after it: it is counted as pending and the interval
//! doubled about the middle. So the interval never gets narrower than a
//! quarter, and a split never rounds either part away.
//!
//! The stream is the bits of one number inside the final interval:
//! enough of them that any bits after them -- the reader reads 0 past
//! the end -- stay inside it. Decoding replays every split with the
//! same odds, and reads each bit off which part that number is in.

use super::bit_stream::{BitReader, BitStream};

/// Bits the interval's ends are held to.
const PRECISION: u32 = 32;
/// The interval's upper end, open: every number is below it.
const WHOLE: u64 = 1 << PRECISION;
/// Half of it...
const HALF: u64 = WHOLE / 2;
/// ...and a quarter.
const QUARTER: u64 = WHOLE / 4;

/// The odds of a bit, as how many times it was clear and how many set,
/// in any unit: the interval is split in that ratio. Neither is ever 0.
#[derive(Clone, Copy, Debug)]
pub struct Odds {
    /// Weight of clear.
    pub clear: u32,
    /// Weight of set.
    pub set: u32,
}

/// The most two weights of [`Odds`] may add up to: the interval is
/// never narrower than a quarter, so a part of weight 1 in this never
/// rounds away.
pub const MOST_WEIGHT: u64 = QUARTER;

impl Odds {
    /// The highest number of the clear part of `low..=high`: clear keeps
    /// `low..=split`, set `split + 1..=high`.
    #[inline]
    fn split(self, low: u64, high: u64) -> u64 {
        let total = self.clear as u64 + self.set as u64;
        debug_assert!(self.clear > 0 && self.set > 0 && total <= MOST_WEIGHT, "{self:?}");
        low + (high - low + 1) * self.clear as u64 / total - 1
    }
}

/// Where coded bits go: a stream, or a count of them.
pub trait BitSink {
    /// Takes one bit.
    fn push_bit(&mut self, bit: bool);
}

impl BitSink for BitStream {
    fn push_bit(&mut self, bit: bool) {
        self.push(bit);
    }
}

/// Counts bits without keeping them.
#[derive(Default)]
pub struct BitCount(pub u64);

impl BitSink for BitCount {
    fn push_bit(&mut self, _: bool) {
        self.0 += 1;
    }
}

/// Bits [`Encoder::finish`] writes at most beyond the pending ones: the
/// one settling the last pending bits, and the one after.
pub const FINISHING_BITS: usize = 2;

/// Codes bits into a sink.
pub struct Encoder {
    /// The interval's lower end.
    low: u64,
    /// Its upper end, inclusive.
    high: u64,
    /// Bits settled only once the next one is: each the opposite of it.
    pending: u64,
}

impl Default for Encoder {
    /// The whole interval, nothing coded.
    fn default() -> Self {
        Self { low: 0, high: WHOLE - 1, pending: 0 }
    }
}

impl Encoder {
    /// Codes `bit` at `odds`.
    #[inline]
    pub fn encode(&mut self, bit: bool, odds: Odds, sink: &mut impl BitSink) {
        let split = odds.split(self.low, self.high);
        if bit {
            self.low = split + 1;
        } else {
            self.high = split;
        }
        loop {
            if self.high < HALF {
                self.settle(false, sink);
            } else if self.low >= HALF {
                self.settle(true, sink);
                self.low -= HALF;
                self.high -= HALF;
            } else if self.low >= QUARTER && self.high < HALF + QUARTER {
                self.pending += 1;
                self.low -= QUARTER;
                self.high -= QUARTER;
            } else {
                break;
            }
            self.low <<= 1;
            self.high = self.high << 1 | 1;
        }
    }

    /// Writes `bit`, then every pending bit, each its opposite.
    fn settle(&mut self, bit: bool, sink: &mut impl BitSink) {
        sink.push_bit(bit);
        for _ in 0..self.pending {
            sink.push_bit(!bit);
        }
        self.pending = 0;
    }

    /// Ends the stream: the fewest bits that keep any number starting
    /// with them inside the interval. It straddles the middle, and holds
    /// a whole quarter on one side of it: the second quarter when it
    /// starts below it, else the third.
    pub fn finish(mut self, sink: &mut impl BitSink) {
        self.pending += 1;
        self.settle(self.low >= QUARTER, sink);
    }
}

/// Reads back bits an [`Encoder`] coded.
pub struct Decoder {
    /// The interval's lower end.
    low: u64,
    /// Its upper end, inclusive.
    high: u64,
    /// The number the stream spells, as far as the interval's precision
    /// reaches.
    value: u64,
}

impl Decoder {
    /// Starts reading at `reader`'s next bit.
    pub fn new(reader: &mut BitReader) -> Self {
        let mut value = 0;
        for _ in 0..PRECISION {
            value = value << 1 | reader.bit() as u64;
        }
        Self { low: 0, high: WHOLE - 1, value }
    }

    /// Reads a bit coded at `odds`.
    #[inline]
    pub fn decode(&mut self, odds: Odds, reader: &mut BitReader) -> bool {
        let split = odds.split(self.low, self.high);
        let bit = self.value > split;
        if bit {
            self.low = split + 1;
        } else {
            self.high = split;
        }
        loop {
            if self.high < HALF {
            } else if self.low >= HALF {
                self.low -= HALF;
                self.high -= HALF;
                self.value -= HALF;
            } else if self.low >= QUARTER && self.high < HALF + QUARTER {
                self.low -= QUARTER;
                self.high -= QUARTER;
                self.value -= QUARTER;
            } else {
                break;
            }
            self.low <<= 1;
            self.high = self.high << 1 | 1;
            self.value = self.value << 1 | reader.bit() as u64;
        }
        bit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Codes `bits`, each at the odds `odds` gives it, and reads them back.
    fn round_trip(bits: &[bool], odds: impl Fn(usize) -> Odds) -> usize {
        let mut stream = BitStream::default();
        let mut encoder = Encoder::default();
        for (index, &bit) in bits.iter().enumerate() {
            encoder.encode(bit, odds(index), &mut stream);
        }
        encoder.finish(&mut stream);
        let mut reader = stream.reader();
        let mut decoder = Decoder::new(&mut reader);
        for (index, &bit) in bits.iter().enumerate() {
            assert_eq!(decoder.decode(odds(index), &mut reader), bit, "bit {index}");
        }
        stream.len()
    }

    #[test]
    fn expected_bits_cost_little_and_surprises_round_trip() {
        let bits: Vec<bool> = (0..4000).map(|index| index % 97 == 0).collect();
        let odds = Odds { clear: 96, set: 1 };
        let written = round_trip(&bits, |_| odds);
        // About 4000 bits at 1/97: some 250 bits of information.
        assert!(written < 400, "{written} bits");
    }

    #[test]
    fn extreme_odds_round_trip() {
        let bits: Vec<bool> = (0..3000).map(|index| (index * 7919) % 13 < 6).collect();
        let heavy = MOST_WEIGHT as u32 - 1;
        round_trip(&bits, |index| if index % 3 == 0 { Odds { clear: heavy, set: 1 } } else { Odds { clear: 1, set: heavy } });
    }

    #[test]
    fn nothing_coded_but_a_finish_is_two_bits() {
        let mut stream = BitStream::default();
        Encoder::default().finish(&mut stream);
        assert_eq!(stream.len(), FINISHING_BITS);
    }
}
