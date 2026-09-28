//! The costs pyramid: for every tile down to 4x4, its bits under each
//! candidate resolution that can be above it -- what the complex tiler
//! counts once a search area and reads while scoring
//! ([`crate::gct::complex_tiler::cost_pyramid`]).
//!
//! One element a tile, one slot a candidate: slot [`NO_CANDIDATE`] the
//! tile's bits with no candidate above it, slot `r` its bits with a
//! complex tile of resolution `r` above it. Nine 21-bit counts, three a
//! word, three words a tile -- then the nodes the tile's count with no
//! candidate visits, at each level and coarser ([`NodeCounts`]), two
//! words: five words a tile, all of a tile's counts, and its four
//! children's, together in Morton order.

use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::grammar::bit_stream::MOST_BITS;
use crate::gct::tile::{tiles_down_to, Tile, CELL_LEVEL};

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

/// Levels a node can be at: the whole bitmap's to the 2x2 floor's.
const NODE_LEVELS: usize = CELL_LEVEL as usize;
/// Bits a level's node count takes: enough for every node there is.
const NODE_COUNT_BITS: usize = 16;
const _: () = assert!(tiles_down_to(CELL_LEVEL - 1) < 1 << NODE_COUNT_BITS);
/// Node counts a word.
const NODE_COUNTS_A_WORD: usize = u64::BITS as usize / NODE_COUNT_BITS;
/// Words a tile's node counts take.
const NODE_WORDS: usize = NODE_LEVELS / NODE_COUNTS_A_WORD;
const _: () = assert!(NODE_LEVELS % NODE_COUNTS_A_WORD == 0);
/// One node count, at the bottom of a word.
const NODE_COUNT_MASK: u64 = (1 << NODE_COUNT_BITS) - 1;

/// Every slot of a tile, then its node counts, in whole words.
const SHAPE: PyramidShape =
    PyramidShape { coarsest_level: 0, finest_level: FINEST_HELD, element_bits: (WORDS + NODE_WORDS) * u64::BITS as usize };

/// The nodes a count visits under a tile, the tile's own included: for
/// each level, how many at that level or coarser -- so the nodes a
/// complex tile of resolution `r` adds a mask bit to are one read. Kept
/// a level to a 16-bit lane, so two of them add lane by lane as two
/// words.
#[derive(Clone, Copy, Default)]
pub struct NodeCounts([u64; NODE_WORDS]);

impl NodeCounts {
    /// One node, at `level`: counted at that level and every finer one.
    #[inline]
    pub fn one_at(level: u8) -> Self {
        let mut words = [0; NODE_WORDS];
        for lane in level as usize..NODE_LEVELS {
            words[lane / NODE_COUNTS_A_WORD] += 1 << (lane % NODE_COUNTS_A_WORD * NODE_COUNT_BITS);
        }
        NodeCounts(words)
    }

    /// Adds `other`'s nodes, lane by lane: no lane ever carries into the
    /// next, since no count reaches a lane's limit.
    #[inline]
    pub fn add(&mut self, other: NodeCounts) {
        for (word, other) in self.0.iter_mut().zip(other.0) {
            *word += other;
        }
    }

    /// How many nodes are at `level` or coarser.
    #[inline]
    pub fn up_to(&self, level: u8) -> u64 {
        let lane = level as usize;
        self.0[lane / NODE_COUNTS_A_WORD] >> (lane % NODE_COUNTS_A_WORD * NODE_COUNT_BITS) & NODE_COUNT_MASK
    }
}

/// One tile's counts, packed as its element holds them, to be set at once.
#[derive(Clone, Copy, Default)]
pub struct Counts {
    /// Its slots.
    slots: [u64; WORDS],
    /// Its node counts.
    nodes: NodeCounts,
}

impl Counts {
    /// Puts `bits` in `slot`, which held nothing.
    #[inline]
    pub fn put(&mut self, slot: u8, bits: u64) {
        debug_assert!(bits <= COUNT_MASK, "{bits} bits do not fit a count");
        let slot = slot as usize;
        self.slots[slot / COUNTS_A_WORD] |= bits << (slot % COUNTS_A_WORD * COUNT_BITS);
    }

    /// Puts the tile's node counts.
    #[inline]
    pub fn put_nodes(&mut self, nodes: NodeCounts) {
        self.nodes = nodes;
    }
}

/// The costs pyramid's queries and updates.
pub trait Costs {
    /// Room for every tile's counts, nothing counted.
    fn costs() -> Self;

    /// `tile`'s count in `slot`.
    fn count(&self, tile: Tile, slot: u8) -> u64;

    /// `tile`'s node counts.
    fn nodes(&self, tile: Tile) -> NodeCounts;

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
    fn nodes(&self, tile: Tile) -> NodeCounts {
        let words = self.element_words(tile);
        NodeCounts([words[WORDS], words[WORDS + 1]])
    }

    #[inline]
    fn set_counts(&mut self, tile: Tile, counts: Counts) {
        let words = self.element_words_mut(tile);
        words[..WORDS].copy_from_slice(&counts.slots);
        words[WORDS..].copy_from_slice(&counts.nodes.0);
    }
}
