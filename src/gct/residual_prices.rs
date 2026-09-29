//! What the last pass takes for each residual block: the bits its cells
//! cost, each at the odds its context had -- the complex tiler's price
//! for leaving a 4x4 to the last pass, in place of a bit a cell.
//!
//! Measured on the greedy tiler's own tree, before the complex tiler
//! runs, by a pass that prices without coding (`Pricing`, in the last
//! pass), as the greedy tiler reaches each block. A residual block costs
//! about the same whatever else the complex tiler changes: a cell's
//! context is the cells above and left of it, which hold the same values
//! whichever node says them -- only the odds each context has learned by
//! then differ. And every residual block the complex tiler can leave is
//! one in the greedy tiler's tree: it only ever adds complex tiles.

use crate::gct::last_pass::BLOCKS;
use crate::gct::tile::{Tile, FLOOR_LEVEL};
use crate::morton::morton_index;

/// Each residual block's bits in the last pass, by its Morton index
/// among the 4x4 blocks: rounded to the nearest bit, as the counts the
/// complex tiler makes are whole bits.
pub struct ResidualPrices {
    /// The bits, by block.
    bits: Box<[u16; BLOCKS]>,
}

impl ResidualPrices {
    /// Room for every block's price, none measured yet.
    pub fn new() -> Self {
        Self { bits: Box::new([0; BLOCKS]) }
    }

    /// What the last pass took for the residual block at `block`, a
    /// 4x4.
    pub fn of(&self, block: Tile) -> u64 {
        debug_assert_eq!(block.level, FLOOR_LEVEL, "{block:?} is not a 4x4 block");
        self.of_index(morton_index(block.x, block.y))
    }

    /// What the last pass took for the residual block at Morton index
    /// `index` among the 4x4 blocks.
    pub fn of_index(&self, index: usize) -> u64 {
        self.bits[index] as u64
    }

    /// Notes that the residual block at `index` took `bits`, in
    /// [`FRACTION_BITS`] fixed point.
    pub(crate) fn set(&mut self, index: usize, bits: u32) {
        self.bits[index] = ((bits + HALF_A_BIT) >> FRACTION_BITS) as u16;
    }
}

/// Bits of a fixed-point `log2` below the point: a 256th of a bit, far
/// finer than the whole bits a price is rounded to.
pub(crate) const FRACTION_BITS: u32 = 8;
/// Half a bit, in fixed point: what rounding to the nearest bit adds.
const HALF_A_BIT: u32 = 1 << (FRACTION_BITS - 1);

/// `log2(1 + m / 256)` in fixed point, for every `m` below 256: the
/// fraction's bits found one at a time by squaring, when compiling.
const FRACTIONS: [u32; 1 << FRACTION_BITS] = {
    // The mantissa in 2.30 fixed point, squared a fraction bit at a time.
    const POINT: u32 = 30;
    let mut fractions = [0; 1 << FRACTION_BITS];
    let mut m = 0;
    while m < fractions.len() {
        let mut mantissa: u64 = (fractions.len() as u64 + m as u64) << (POINT - FRACTION_BITS);
        let mut fraction = 0;
        let mut bit = 0;
        while bit < FRACTION_BITS {
            mantissa = (mantissa * mantissa) >> POINT;
            fraction <<= 1;
            if mantissa >= 2 << POINT {
                mantissa >>= 1;
                fraction |= 1;
            }
            bit += 1;
        }
        fractions[m] = fraction;
        m += 1;
    }
    fractions
};

/// `log2(value)` in [`FRACTION_BITS`] fixed point, `value` at least 1:
/// its whole part, and its fraction from the mantissa's top bits -- the
/// value moved up until its leading one is the top bit, then the bits
/// under it. Wide enough for the product of a block row's odds.
#[inline]
pub(crate) fn fixed_point_log2(value: u128) -> u32 {
    let whole = value.ilog2();
    let normalized = value << (u128::BITS - 1 - whole);
    let mantissa_top = (normalized >> (u128::BITS - 1 - FRACTION_BITS)) as usize & ((1 << FRACTION_BITS) - 1);
    whole << FRACTION_BITS | FRACTIONS[mantissa_top]
}

impl Default for ResidualPrices {
    /// The same as [`ResidualPrices::new`].
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixed-point `log2` is never above the true one, and under a
    /// hundredth of a bit below it -- the mantissa's bits past the top
    /// eight, and the fraction's past the eighth, both cut off -- from 1
    /// to the most a context's weights reach, and through products of
    /// them.
    #[test]
    fn fixed_point_log2_is_close() {
        let hundredth_of_a_bit = (1 << FRACTION_BITS) as f64 / 100.0;
        for value in (1..1u64 << 18).step_by(7).chain((1..1u64 << 60).step_by(1 << 44)) {
            let exact = (value as f64).log2() * (1 << FRACTION_BITS) as f64;
            let fixed = fixed_point_log2(value as u128) as f64;
            assert!(fixed <= exact && exact - fixed < hundredth_of_a_bit, "log2({value}): {fixed} against {exact}");
        }
    }
}
