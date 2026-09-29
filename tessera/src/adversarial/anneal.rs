//! Simulated annealing: try a change, keep it if it raises the score,
//! and early on sometimes keep one that lowers it -- less and less
//! often as the search cools -- so it can climb out of a local best.
//! The best bitmap seen is what it returns.
//!
//! The kinds of change learn: each is drawn in proportion to one plus
//! the times it has raised the score, so a search leans on whatever is
//! working on the bitmap in front of it.
//!
//! What is maximized is the caller's: any score of a bitmap.

use super::moves::CHANGES;
use utilities::rng::Rng;
use super::Score;
use crate::tile::Tile;
use bitmap::Bitmap;

/// How many bits a change may lose and still often be kept at the
/// start: about what one change moves, so early on the search crosses
/// small valleys freely. It cools linearly to nothing.
const START_TEMPERATURE: f64 = 8.0;

/// The worst bitmap a search found, and its score.
pub struct Found {
    /// The bitmap.
    pub bitmap: Bitmap,
    /// Its score.
    pub score: Score,
}

/// A kind of change, drawn in proportion to one plus its successes.
fn pick(rng: &mut Rng, successes: &[u64]) -> usize {
    let mut left = rng.below(successes.iter().map(|&count| count + 1).sum());
    for (kind, &count) in successes.iter().enumerate() {
        if left <= count {
            return kind;
        }
        left -= count + 1;
    }
    unreachable!("the draw is below the sum")
}

/// The best bitmap `iterations` changes inside `area` reach from
/// `start`, by `score`.
pub fn anneal(start: Bitmap, area: Tile, iterations: u64, rng: &mut Rng, score: &mut impl FnMut(&Bitmap) -> Score) -> Found {
    let mut successes = [0; CHANGES.len()];
    let mut current = start;
    let mut current_score = score(&current);
    let mut best = Found { bitmap: current.clone(), score: current_score };
    for iteration in 0..iterations {
        let temperature = START_TEMPERATURE * (1.0 - iteration as f64 / iterations as f64);
        let kind = pick(rng, &successes);
        let mut next = current.clone();
        CHANGES[kind](rng, &mut next, area);
        let next_score = score(&next);
        let gain = (next_score.gap - current_score.gap) as f64;
        if gain > 0.0 {
            successes[kind] += 1;
        }
        if gain >= 0.0 || (temperature > 0.0 && rng.unit() < (gain / temperature).exp()) {
            current = next;
            current_score = next_score;
            if current_score.gap > best.score.gap {
                best = Found { bitmap: current.clone(), score: current_score };
            }
        }
    }
    best
}
