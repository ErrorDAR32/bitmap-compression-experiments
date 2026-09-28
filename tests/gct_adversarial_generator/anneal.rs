//! Simulated annealing: try a change, keep it if it raises the score,
//! and early on sometimes keep one that lowers it -- less and less
//! often as the search cools -- so it can climb out of a local best.
//! The best bitmap seen is what it returns.
//!
//! The kinds of change learn: each starts equally likely, and each time
//! one raises the score it becomes likelier, so a search leans on
//! whatever is working on the bitmap in front of it.

use super::moves::CHANGES;
use super::objectives::{Objective, Score, Scorer};
use super::rng::Rng;
use bitmap::gct::tile::Tile;
use bitmap::Bitmap;

/// How many bits a change may lose and still often be kept at the
/// start: about what one change moves, so early on the search crosses
/// small valleys freely. It cools linearly to nothing.
const START_TEMPERATURE: f64 = 8.0;

/// How much likelier a kind of change becomes each time it raises the
/// score, against the one weight every kind starts with.
const WEIGHT_PER_SUCCESS: f64 = 1.0;
const STARTING_WEIGHT: f64 = 1.0;

pub struct Found {
    pub bitmap: Bitmap,
    pub score: Score,
}

/// A kind of change, drawn by weight.
fn pick(rng: &mut Rng, weights: &[f64]) -> usize {
    let mut left = rng.unit() * weights.iter().sum::<f64>();
    for (kind, &weight) in weights.iter().enumerate() {
        if left < weight {
            return kind;
        }
        left -= weight;
    }
    weights.len() - 1
}

/// The best bitmap `iterations` changes inside `area` reach from `start`.
pub fn anneal(
    start: Bitmap,
    area: Tile,
    objective: Objective,
    iterations: u64,
    rng: &mut Rng,
    scorer: &mut Scorer,
) -> Found {
    let mut weights = [STARTING_WEIGHT; CHANGES.len()];
    let mut current = start;
    let mut current_score = scorer.score(objective, &current, area);
    let mut best = Found { bitmap: current.clone(), score: current_score };
    for iteration in 0..iterations {
        let temperature = START_TEMPERATURE * (1.0 - iteration as f64 / iterations as f64);
        let kind = pick(rng, &weights);
        let mut next = current.clone();
        CHANGES[kind](rng, &mut next, area);
        let next_score = scorer.score(objective, &next, area);
        let gain = (next_score.gap - current_score.gap) as f64;
        if gain > 0.0 {
            weights[kind] += WEIGHT_PER_SUCCESS;
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
