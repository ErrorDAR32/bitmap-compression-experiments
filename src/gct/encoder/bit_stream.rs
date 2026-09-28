//! A run of bits: written in order, read back in the same order.
//! Multi-bit values go least significant bit first. One bit to an
//! entry, nothing packed -- the length is the vector's own.

#[derive(Default, Clone, PartialEq, Eq, Debug)]
pub struct BitStream {
    bits: Vec<bool>,
}

impl BitStream {
    /// How many bits have been written.
    pub fn len(&self) -> usize {
        self.bits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bits.is_empty()
    }

    /// The bits, in the order written.
    pub fn bits(&self) -> &[bool] {
        &self.bits
    }

    /// Writes one bit.
    pub fn push(&mut self, bit: bool) {
        self.bits.push(bit);
    }

    /// Writes the low `width` bits of `value`.
    pub fn push_value(&mut self, value: u64, width: usize) {
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
    stream: &'a BitStream,
    at: usize,
}

impl BitReader<'_> {
    /// Reads one bit.
    pub fn bit(&mut self) -> bool {
        let bit = self.stream.bits.get(self.at).copied().unwrap_or(false);
        self.at += 1;
        bit
    }

    /// Reads `width` bits, as written by [`BitStream::push_value`].
    pub fn value(&mut self, width: usize) -> u64 {
        (0..width).fold(0, |value, bit| value | (self.bit() as u64) << bit)
    }
}
