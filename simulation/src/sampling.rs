//! Monte Carlo sampling of the hot bitplanes (`../bitplane_manager`): every set cell of a layer
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
//! passed over whole, a chunk by its count, a block of 16 words by its
//! count, a word by its bits' count, and only the word holding a chosen
//! cell is searched. So a sample costs the same few counts however far
//! from the last it is: the rarer the samples, the less of a bitmap is
//! read at all.

use bitmap::BITS_PER_WORD;
use bitplane_manager::{BitmapArena, LayerView, BLOCK_WORDS};
use coordinates::{CellIndex, CHUNKS_IN_SUPERCHUNK};
use chunk_storage::LayerType;
use utilities::rng::Rng;

/// Draws how many set cells to pass over before the next chosen one,
/// each chosen with the probability whose complement's natural
/// logarithm is `log_unchosen`.
fn gap(random: &mut Rng, log_unchosen: f64) -> u64 {
    // In (0, 1]: never 0, so its logarithm is finite.
    let uniform = 1.0 - random.unit();
    let gap = (uniform.ln() / log_unchosen).floor();
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

/// Chooses each hot set cell of `layer` -- of the superchunk whose
/// Morton index is `superchunk` -- with `probability`, independently,
/// and hands every chosen cell to `emit` in Morton order: how many were
/// chosen. A probability of 1 or more chooses every set cell; 0 or
/// less, none.
pub fn sample_layer(superchunk: u64, layer: LayerView, probability: f64, random: &mut Rng, emit: &mut impl FnMut(CellIndex)) -> usize {
    if probability <= 0.0 || layer.hot_count() == 0 {
        return 0;
    }
    let log_unchosen = (1.0 - probability.min(1.0)).ln();
    let draw = |random: &mut Rng| if probability >= 1.0 { 0 } else { gap(random, log_unchosen) };
    // The rank, among the layer's hot set cells still ahead, of the next
    // one chosen.
    let mut next = draw(random);
    if next >= layer.hot_count() as u64 {
        return 0;
    }
    let mut chosen = 0;
    for chunk in 0..CHUNKS_IN_SUPERCHUNK {
        if !layer.is_hot(chunk) {
            continue;
        }
        let count = layer.count(chunk) as u64;
        if next >= count {
            next -= count;
            continue;
        }
        let (cells, blocks) = (layer.cells(chunk), layer.block_counts(chunk));
        // Set cells in the words before `word`, and in those before its block.
        let (mut word, mut before, mut before_block) = (0, 0u64, 0u64);
        while next < count {
            // The blocks before the chosen cell's passed over whole, by their counts...
            loop {
                let after_block = before_block + blocks[word / BLOCK_WORDS] as u64;
                if next < after_block {
                    break;
                }
                (before_block, before, word) = (after_block, after_block, (word / BLOCK_WORDS + 1) * BLOCK_WORDS);
            }
            // ...then the words before it in its block, by their bits'.
            loop {
                let ones = cells[word].count_ones() as u64;
                if before + ones > next {
                    break;
                }
                before += ones;
                word += 1;
            }
            let bit = select(cells[word], (next - before) as u32);
            emit(CellIndex::from_parts(superchunk, chunk, word * BITS_PER_WORD + bit as usize));
            chosen += 1;
            next += 1 + draw(random);
        }
        next -= count;
    }
    chosen
}

/// Chooses each set cell of `layer_type`'s hot bitmaps in `arena` with
/// `probability`, independently, and hands every chosen cell to `emit`
/// in Morton order -- superchunk by superchunk: how many were chosen.
pub fn sample(arena: &BitmapArena, layer_type: LayerType, probability: f64, random: &mut Rng, mut emit: impl FnMut(CellIndex)) -> usize {
    arena
        .superchunks()
        .iter()
        .filter_map(|superchunk| superchunk.layer(layer_type).map(|layer| (superchunk.morton_index(), layer)))
        .map(|(superchunk, layer)| sample_layer(superchunk, layer, probability, random, &mut emit))
        .sum()
}
