//! An encoded bitmap: the bits an encode produced, in the order it
//! produced them.
//!
//! One file, because it is one idea.
//!
//! One bit to an entry. Nothing here packs anything, and the length is
//! the vector's own length rather than a counter kept beside it --
//! there is no second number to drift out of step with the first, and
//! no arithmetic between "how many bits" and "how many words" for a
//! reader to get wrong.

/// A run of bits, written in order and read back by position.
#[derive(Default, Clone)]
pub struct EncodedBitmap {
    bits: Vec<bool>,
}

impl EncodedBitmap {
    /// How many bits have been written.
    pub fn len(&self) -> usize {
        self.bits.len()
    }

    /// Whether nothing has been written.
    pub fn is_empty(&self) -> bool {
        self.bits.is_empty()
    }

    pub fn clear(&mut self) {
        self.bits.clear();
    }

    /// Writes one bit.
    pub fn push(&mut self, bit: bool) {
        self.bits.push(bit);
    }

    /// Writes the low `width` bits of a value, least significant
    /// first, which is the order [`EncodedBitmap::take`] reads them.
    pub fn push_value(&mut self, value: u64, width: usize) {
        for bit in 0..width {
            self.push(value >> bit & 1 == 1);
        }
    }

    /// The bit at a position, or `false` past the end.
    pub fn at(&self, position: usize) -> bool {
        self.bits.get(position).copied().unwrap_or(false)
    }

    /// `width` bits from `position`, least significant first.
    pub fn take(&self, position: usize, width: usize) -> u64 {
        let mut value = 0;
        for bit in 0..width {
            if self.at(position + bit) {
                value |= 1 << bit;
            }
        }
        value
    }
}
