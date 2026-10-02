//! Monte Carlo sampling of the hot bitplanes: every set cell of a layer
//! type chosen with one probability, independently, and handed out in
//! Morton order -- superchunk by superchunk, chunk by chunk, cell by
//! cell -- so what is computed from the samples, and the writes it
//! queues, come in that order already, never sorted.
//!
//! No sample is wasted: the cells are not each tossed a coin, nor drawn
//! and rejected. The set cells are ranked in Morton order, and the gap
//! from one chosen rank to the next is drawn from the geometric law
//! (`docs/tilesim.md`, "Sampling"): each set cell is then chosen with
//! the probability asked, and only the chosen ones are found. The
//! counts find them: a superchunk bitplane with no hot cell set is
//! passed over whole, a chunk by its count, a word by its bits' count,
//! and only the word holding a chosen cell is searched.

use crate::random::Random;
use crate::{bucket_in, contains, BitmapArena};
use bitmap::BITS_PER_WORD;
use chunk_storage::{CellIndex, LayerType, CHUNKS_IN_SUPERCHUNK};

/// Draws how many set cells to pass over before the next chosen one,
/// each chosen with the probability whose complement's natural
/// logarithm is `log_unchosen`.
fn gap(random: &mut Random, log_unchosen: f64) -> u64 {
    let gap = (random.unit().ln() / log_unchosen).floor();
    if gap < u64::MAX as f64 / 2.0 { gap as u64 } else { u64::MAX / 2 }
}

/// The position of the `rank`-th set bit of `word`, counting from 0 at
/// the lowest; `word` has more set bits than `rank`.
fn select(mut word: u64, rank: u32) -> u32 {
    for _ in 0..rank {
        word &= word - 1;
    }
    word.trailing_zeros()
}

impl BitmapArena {
    /// Chooses each set cell of `layer_type`'s hot bitmaps with
    /// `probability`, independently, and hands every chosen cell to
    /// `emit` in Morton order: how many were chosen. A probability of 1
    /// or more chooses every set cell; 0 or less, none.
    pub fn sample(&self, layer_type: LayerType, probability: f64, random: &mut Random, mut emit: impl FnMut(CellIndex)) -> usize {
        if probability <= 0.0 {
            return 0;
        }
        let log_unchosen = (1.0 - probability.min(1.0)).ln();
        let draw = |random: &mut Random| if probability >= 1.0 { 0 } else { gap(random, log_unchosen) };
        let mut chosen = 0;
        for (superchunk, allocation) in self.layers_of(layer_type) {
            if allocation.hot_count == 0 {
                continue;
            }
            // The rank, among the allocation's hot set cells still ahead,
            // of the next one chosen.
            let mut next = draw(random);
            if next >= allocation.hot_count as u64 {
                continue;
            }
            let words = self.pool.block(allocation.block);
            for index in 0..CHUNKS_IN_SUPERCHUNK {
                if !contains(allocation.flags.hot, index) {
                    continue;
                }
                let count = allocation.count(index) as u64;
                if next >= count {
                    next -= count;
                    continue;
                }
                let cells = bucket_in(words, index);
                // Set cells in the words before `word`.
                let (mut word, mut before) = (0, 0u64);
                while next < count {
                    loop {
                        let ones = cells[word].count_ones() as u64;
                        if before + ones > next {
                            break;
                        }
                        before += ones;
                        word += 1;
                    }
                    let bit = select(cells[word], (next - before) as u32);
                    emit(CellIndex::of(superchunk, index, word * BITS_PER_WORD + bit as usize));
                    chosen += 1;
                    next += 1 + draw(random);
                }
                next -= count;
            }
        }
        chosen
    }
}
