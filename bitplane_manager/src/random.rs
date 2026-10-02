//! Random numbers for sampling: xorshift64*, fast and good enough to
//! pick cells by, seeded so a run can be repeated.

/// A xorshift64* generator.
#[derive(Clone, Debug)]
pub struct Random(u64);

impl Random {
    /// A generator from `seed`; any seed works, 0 included.
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15 | 1)
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number below `bound`, which is not 0.
    pub fn below(&mut self, bound: u32) -> u32 {
        (((self.next_u64() >> 32) * bound as u64) >> 32) as u32
    }

    /// A number in `(0, 1]`: never 0, so its logarithm is finite.
    pub fn unit(&mut self) -> f64 {
        ((self.next_u64() >> 11) + 1) as f64 / (1u64 << 53) as f64
    }
}
