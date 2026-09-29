//! The costs pyramid: for every tile down to 4x4, its bits under every
//! candidate resolution that can be above it -- what the complex tiler
//! counts once a search area and reads while scoring
//! ([`crate::gct::complex_tiler::cost_pyramid`]).
//!
//! One element a tile, eight 32-bit slots: its bits with no candidate
//! above it, then for each resolution `r` finer than the tile how much a
//! complex tile of resolution `r` above it takes off that -- its change,
//! negative when the candidate adds bits. Its bits under the candidate
//! are the first less the change. A resolution coarser than the tile
//! changes nothing, and the change at the tile's own level follows from
//! the tile alone, so neither is held: seven slots are enough for the
//! seven finer levels a tile has at most. A tile's four children's
//! elements are together, in Morton order.

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

/// Bits a slot takes: the bits with no candidate, or one change.
const SLOT_BITS: usize = u32::BITS as usize;
/// Slots an element holds: the bits with no candidate, and a change for
/// each resolution finer than the tile, up to seven.
const SLOTS: usize = 8;
/// The slot of the bits with no candidate; a change's is how many levels
/// finer than the tile its resolution is.
const WITHOUT_SLOT: usize = 0;
/// The most levels finer than its tile a held change's resolution is.
const FINEST_HELD_CHANGE: u8 = (SLOTS - 1) as u8;
/// A tile's bits with no candidate fit a slot: they are fewer than a
/// stream's.
const _: () = assert!(crate::gct::grammar::bit_stream::MOST_BITS <= u32::MAX as usize);

/// Slots a word.
const SLOTS_A_WORD: usize = u64::BITS as usize / SLOT_BITS;

/// Eight 32-bit slots a tile, whole bitmap to 4x4.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CostsShape;

impl PyramidShape for CostsShape {
    const COARSEST_LEVEL: u8 = 0;
    const FINEST_LEVEL: u8 = FINEST_HELD;
    const ELEMENT_BITS: usize = SLOTS * SLOT_BITS;
}

/// How much a candidate of each resolution takes off a tile's bits, by
/// resolution: index `r - 1` for resolution `r`.
pub type Changes = [i32; RESOLUTIONS];

/// The costs pyramid: each tile's bits with no candidate, and its
/// changes.
pub type Costs = Pyramid<CostsShape, { CostsShape::WORDS }>;

impl Costs {
    /// `tile`'s bits with no candidate above it.
    #[inline]
    pub fn without(&self, tile: Tile) -> u64 {
        slot(self.element_words(tile), WITHOUT_SLOT) as u64
    }

    /// How much a candidate of `resolution`, finer than `tile`, above it
    /// takes off its bits.
    #[inline]
    pub fn change(&self, tile: Tile, resolution: u8) -> i32 {
        slot(self.element_words(tile), change_slot(tile, resolution)) as i32
    }

    /// `tile`'s changes for every resolution finer than it; `0` for the
    /// others, which are not held.
    #[inline]
    pub fn finer_changes(&self, tile: Tile) -> Changes {
        let words = self.element_words(tile);
        std::array::from_fn(|change_index| {
            let resolution = change_index as u8 + 1;
            if resolution > tile.level { slot(words, change_slot(tile, resolution)) as i32 } else { 0 }
        })
    }

    /// Sets all of `tile`'s counts at once: `changes` for every resolution
    /// finer than it are held, up to seven levels finer -- only the whole
    /// bitmap has one finer still, 1x1, and it is no tile's child, so its
    /// changes are never read.
    #[inline]
    pub fn set_counts(&mut self, tile: Tile, without: u64, changes: &Changes) {
        let words = self.element_words_mut(tile);
        set_slot(words, WITHOUT_SLOT, without as u32);
        let finest = (tile.level + FINEST_HELD_CHANGE).min(CELL_LEVEL);
        for resolution in tile.level + 1..=finest {
            set_slot(words, change_slot(tile, resolution), changes[resolution as usize - 1] as u32);
        }
    }
}

/// The slot of `tile`'s change for `resolution`, finer than it.
#[inline]
fn change_slot(tile: Tile, resolution: u8) -> usize {
    debug_assert!(resolution > tile.level && resolution - tile.level <= FINEST_HELD_CHANGE, "{tile:?} holds no change for {resolution}");
    (resolution - tile.level) as usize
}

/// Slot `index` of an element's `words`.
#[inline]
fn slot(words: &[u64], index: usize) -> u32 {
    (words[index / SLOTS_A_WORD] >> (index % SLOTS_A_WORD * SLOT_BITS)) as u32
}

/// Sets slot `index` of an element's `words` to `value`.
#[inline]
fn set_slot(words: &mut [u64], index: usize, value: u32) {
    let shift = index % SLOTS_A_WORD * SLOT_BITS;
    let word = &mut words[index / SLOTS_A_WORD];
    *word = *word & !((u32::MAX as u64) << shift) | (value as u64) << shift;
}
