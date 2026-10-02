//! The range coder: what it takes, and that it reads back what it wrote
//! at the most skewed odds and through carries.

use crate::arithmetic::{ClearProbability, Decoder, Encoder, FINISHING_BITS};
use crate::bit_stream::{BitStream, Sink};

/// Codes `bits`, each at the probability `clear` gives it, and reads
/// them back with set bits after the stream -- a stream ends itself:
/// how many bits the stream took.
fn round_trip(bits: &[bool], clear: impl Fn(usize) -> ClearProbability) -> usize {
    let mut stream = BitStream::default();
    let mut encoder = Encoder::default();
    for (index, &bit) in bits.iter().enumerate() {
        encoder.encode(bit, clear(index), &mut stream);
    }
    encoder.finish(&mut stream);
    let written = stream.len();
    stream.push_value(u64::MAX, u64::BITS as u8);
    let mut reader = stream.reader();
    let mut decoder = Decoder::new(&mut reader);
    for (index, &bit) in bits.iter().enumerate() {
        assert_eq!(decoder.decode(clear(index), &mut reader), bit, "bit {index}");
    }
    written
}

/// A 2^32 fraction of `clear` in `total`.
fn fraction(clear: u64, total: u64) -> ClearProbability {
    ClearProbability(((clear << u32::BITS) / total) as u32)
}

/// The stream takes what the bits' probabilities say they carry --
/// `-log2` of each's -- and no more than [`FINISHING_BITS`] and the
/// rounding over it.
#[test]
fn the_stream_takes_the_bits_information_and_little_more() {
    let bits: Vec<bool> = (0..4000).map(|index| index % 97 == 0).collect();
    let clear = fraction(96, 97);
    let written = round_trip(&bits, |_| clear);
    let clear_share = clear.0 as f64 / (1u64 << u32::BITS) as f64;
    let information: f64 = bits.iter().map(|&bit| -(if bit { 1.0 - clear_share } else { clear_share }).log2()).sum();
    // The rounding: under 2^-12 bits a bit coded.
    let most = information + FINISHING_BITS as f64 + bits.len() as f64 / 4096.0;
    assert!((written as f64) <= most, "{written} bits, the information {information:.1}");
}

#[test]
fn the_most_skewed_probabilities_round_trip() {
    let bits: Vec<bool> = (0..3000).map(|index| (index * 7919) % 13 < 6).collect();
    let (rare, total) = (1, 2048);
    round_trip(&bits, |index| if index % 3 == 0 { fraction(total - rare, total) } else { fraction(rare, total) });
}

#[test]
fn carries_through_held_bytes_round_trip() {
    // Long runs of the likelier value push the lower end up against
    // the window's top, holding 0xFF bytes for a carry.
    let bits: Vec<bool> = (0..20000).map(|index| index % 1000 != 999).collect();
    round_trip(&bits, |_| fraction(1, 2048));
    round_trip(&bits, |index| fraction(1 + (index % 2047) as u64, 2048));
}

/// The whole interval is a hair under `2^32` wide, so ending it names
/// its lower half: a bit. The last pass, coding no cell, never ends its
/// coder at all.
#[test]
fn nothing_coded_takes_a_bit() {
    let mut stream = BitStream::default();
    Encoder::default().finish(&mut stream);
    assert_eq!(stream.len(), 1);
}
