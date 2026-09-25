//! A stream of bits, written low end first and read back by offset.
//!
//! An encoding is a few of these and nothing else. Keeping them apart
//! is not tidiness: the tile passes peek two bits past a label to see
//! whether a subdivide follows, so a second code table sharing one
//! stream desynchronises the reader, and the reader then descends past
//! the bottom of the tree.

#[derive(Default)]
pub struct Bits {
    words: Vec<u64>,
    len: usize,
}

impl Bits {
    /// How many bits have been written.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether nothing has been written.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(crate) fn clear(&mut self) {
        self.words.clear();
        self.len = 0;
    }

    pub(crate) fn push(&mut self, value: u64, width: usize) {
        let (at, shift) = (self.len / 64, self.len % 64);
        if at >= self.words.len() {
            self.words.push(0);
        }
        self.words[at] |= value << shift;
        if shift + width > 64 {
            self.words.push(value >> (64 - shift));
        }
        self.len += width;
    }

    /// `width` bits from `at`, or `None` past the end of the stream.
    pub(crate) fn take(&self, at: usize, width: usize) -> Option<u64> {
        if at + width > self.len {
            return None;
        }
        let (word, shift) = (at / 64, at % 64);
        let mask = if width == 64 { u64::MAX } else { (1u64 << width) - 1 };
        let mut got = self.words[word] >> shift;
        if shift + width > 64 {
            got |= self.words[word + 1] << (64 - shift);
        }
        Some(got & mask)
    }
}

