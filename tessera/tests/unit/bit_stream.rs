//! The bit stream: its words and its bytes load back into the same bits.

use crate::bit_stream::{BitStream, Sink};

/// Bits a word holds.
const WORD_BITS: usize = u64::BITS as usize;
/// Bits a byte holds.
const BYTE_BITS: usize = u8::BITS as usize;

/// A stream's words, loaded into another stream, read back the same
/// bits, and 0 past them.
#[test]
fn words_load_back_into_the_same_bits() {
    let mut written = BitStream::default();
    for value in 0..40u64 {
        written.push_value(value * 2_654_435_761 % 1000, 10);
    }
    written.push(true);
    let mut loaded = BitStream::default();
    loaded.push_value(u64::MAX, 64);
    loaded.load_words(written.words());
    assert_eq!(loaded.len(), written.words().len() * WORD_BITS);
    let (mut expected, mut read) = (written.reader(), loaded.reader());
    for bit in 0..written.len() {
        assert_eq!(read.bit(), expected.bit(), "bit {bit}");
    }
    for _ in written.len()..loaded.len() {
        assert!(!read.bit(), "padding reads as 0");
    }
}

/// A stream's bytes, loaded into another stream, read back the same
/// bits, whatever that stream held before -- a stream of whole words
/// and one ending partway through a byte alike.
#[test]
fn bytes_load_back_into_the_same_bits() {
    for extra_bits in [0, 1, 7, 8, 63] {
        let mut written = BitStream::default();
        for value in 0..40u64 {
            written.push_value(value * 2_654_435_761 % 1000, 10);
        }
        for _ in 0..extra_bits {
            written.push(true);
        }
        let bytes = written.to_bytes();
        assert_eq!(bytes.len(), written.len().div_ceil(BYTE_BITS));
        let mut loaded = BitStream::default();
        loaded.push_value(u64::MAX, 64);
        loaded.load_bytes(&bytes);
        assert_eq!(loaded.len(), bytes.len() * BYTE_BITS);
        let (mut expected, mut read) = (written.reader(), loaded.reader());
        for bit in 0..written.len() {
            assert_eq!(read.bit(), expected.bit(), "bit {bit}");
        }
        for _ in written.len()..loaded.len() {
            assert!(!read.bit(), "padding reads as 0");
        }
    }
}
