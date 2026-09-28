//! The costs pyramid: for every tile down to 4x4, its bits under each
//! candidate resolution that can be above it -- what the complex tiler
//! counts once a search area and reads while scoring
//! ([`crate::gct::complex_tiler::cost_pyramid`]).
//!
//! One element a tile, one slot a candidate: slot [`NO_CANDIDATE`] the
//! tile's bits with no candidate above it, slot `r` its bits with a
//! complex tile of resolution `r` above it. Nine 21-bit counts, three a
//! word, three words a tile -- so all of a tile's counts, and its four
//! children's, sit together in Morton order.

use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::grammar::bit_stream::MOST_BITS;
use crate::gct::tile::{Tile, CELL_LEVEL};

/// The finest level held: 4x4, the finest candidate. A 2x2 is counted
/// when asked for.
pub const FINEST_HELD: u8 = CELL_LEVEL - 2;

/// Slots a tile: one for no candidate, then one a resolution.
pub const SLOTS: usize = CELL_LEVEL as usize + 1;
/// The slot for no candidate above.
pub const NO_CANDIDATE: u8 = 0;

/// Bits a count takes: enough for any tile's bits, the whole bitmap's
/// included -- no count is more than the most a stream takes, a
/// candidate above adding a nesting the stream's bound already allows
/// for.
const COUNT_BITS: usize = 21;
const _: () = assert!(MOST_BITS < 1 << COUNT_BITS);
/// Counts a word: a count never straddles two.
const COUNTS_A_WORD: usize = u64::BITS as usize / COUNT_BITS;
/// One count's bits, at the bottom of a word.
const COUNT_MASK: u64 = (1 << COUNT_BITS) - 1;
/// Words a tile's counts take.
const WORDS: usize = SLOTS.div_ceil(COUNTS_A_WORD);

/// Every slot of a tile, in whole words.
const SHAPE: PyramidShape = PyramidShape { coarsest_level: 0, finest_level: FINEST_HELD, element_bits: WORDS * u64::BITS as usize };

/// One tile's counts, packed as its element holds them, to be set at once.
#[derive(Clone, Copy, Default)]
pub struct Counts([u64; WORDS]);

impl Counts {
    /// Puts `bits` in `slot`, which held nothing.
    #[inline]
    pub fn put(&mut self, slot: u8, bits: u64) {
        debug_assert!(bits <= COUNT_MASK, "{bits} bits do not fit a count");
        let slot = slot as usize;
        self.0[slot / COUNTS_A_WORD] |= bits << (slot % COUNTS_A_WORD * COUNT_BITS);
    }
}

/// The costs pyramid's queries and updates.
pub trait Costs {
    /// Room for every tile's counts, nothing counted.
    fn costs() -> Self;

    /// `tile`'s count in `slot`.
    fn count(&self, tile: Tile, slot: u8) -> u64;

    /// Sets all of `tile`'s counts at once.
    fn set_counts(&mut self, tile: Tile, counts: Counts);
}

impl Costs for Pyramid {
    fn costs() -> Self {
        Pyramid::new(SHAPE)
    }

    #[inline]
    fn count(&self, tile: Tile, slot: u8) -> u64 {
        let slot = slot as usize;
        self.element_words(tile)[slot / COUNTS_A_WORD] >> (slot % COUNTS_A_WORD * COUNT_BITS) & COUNT_MASK
    }

    #[inline]
    fn set_counts(&mut self, tile: Tile, counts: Counts) {
        self.element_words_mut(tile).copy_from_slice(&counts.0);
    }
}
