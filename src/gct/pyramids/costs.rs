//! The costs pyramid: for every tile down to 4x4, its bits under every
//! candidate resolution that can be above it -- what the complex tiler
//! counts once a search area and reads while scoring
//! ([`crate::gct::complex_tiler::cost_pyramid`]).
//!
//! One element a tile: its bits with no candidate above it, then for each
//! resolution `r` how much a complex tile of resolution `r` above it takes
//! off that -- its change, negative when the candidate adds bits. Its
//! bits under the candidate are the first less the change. One word, then
//! eight 32-bit changes two a word: five words a tile, a tile's four
//! children's together in Morton order.

use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::complex_tiler::complex_tile_candidates::FINEST_CANDIDATE_LEVEL;
use crate::gct::tile::{Tile, CELL_LEVEL};

/// The finest level held: the finest candidate's. A 2x2 is counted when
/// asked for.
pub const FINEST_HELD: u8 = FINEST_CANDIDATE_LEVEL;

/// The resolution meaning no candidate above.
pub const NO_CANDIDATE: u8 = 0;
/// Candidate resolutions: one level finer than the whole bitmap, to 1x1.
pub const RESOLUTIONS: usize = CELL_LEVEL as usize;

/// Changes a word.
const CHANGES_A_WORD: usize = 2;
/// Bits a change takes.
const CHANGE_BITS: usize = u64::BITS as usize / CHANGES_A_WORD;
/// Words a tile's changes take.
const CHANGE_WORDS: usize = RESOLUTIONS / CHANGES_A_WORD;

/// A tile's bits with no candidate, then its changes, in whole words.
const SHAPE: PyramidShape =
    PyramidShape { coarsest_level: 0, finest_level: FINEST_HELD, element_bits: (1 + CHANGE_WORDS) * u64::BITS as usize };

/// How much a candidate of each resolution takes off a tile's bits, by
/// resolution: index `r - 1` for resolution `r`.
pub type Changes = [i32; RESOLUTIONS];

/// The costs pyramid's queries and updates.
pub trait Costs {
    /// Room for every tile's counts, nothing counted.
    fn costs() -> Self;

    /// `tile`'s bits with no candidate above it.
    fn without(&self, tile: Tile) -> u64;

    /// How much a candidate of `resolution` above `tile` takes off its
    /// bits.
    fn change(&self, tile: Tile, resolution: u8) -> i32;

    /// All of `tile`'s changes.
    fn changes(&self, tile: Tile) -> Changes;

    /// Sets all of `tile`'s counts at once.
    fn set_counts(&mut self, tile: Tile, without: u64, changes: &Changes);
}

impl Costs for Pyramid {
    fn costs() -> Self {
        Pyramid::new(SHAPE)
    }

    #[inline]
    fn without(&self, tile: Tile) -> u64 {
        self.element_words(tile)[0]
    }

    #[inline]
    fn change(&self, tile: Tile, resolution: u8) -> i32 {
        unpacked_change(self.element_words(tile), resolution as usize - 1)
    }

    #[inline]
    fn changes(&self, tile: Tile) -> Changes {
        let words = self.element_words(tile);
        std::array::from_fn(|change_index| unpacked_change(words, change_index))
    }

    #[inline]
    fn set_counts(&mut self, tile: Tile, without: u64, changes: &Changes) {
        let words = self.element_words_mut(tile);
        words[0] = without;
        for (pair_index, pair) in changes.chunks_exact(CHANGES_A_WORD).enumerate() {
            words[1 + pair_index] = pair[0] as u32 as u64 | (pair[1] as u32 as u64) << CHANGE_BITS;
        }
    }
}

/// The change at `change_index` (resolution less one) of an element's
/// `words`: two to a word, after the bits without a candidate.
#[inline]
fn unpacked_change(words: &[u64], change_index: usize) -> i32 {
    (words[1 + change_index / CHANGES_A_WORD] >> (change_index % CHANGES_A_WORD * CHANGE_BITS)) as u32 as i32
}
