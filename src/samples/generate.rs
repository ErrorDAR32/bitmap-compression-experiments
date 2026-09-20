//! How a sample is grown. Free functions over borrowed data: they own
//! nothing, keep nothing between calls, and the whole of what they
//! produce is settled by their arguments.
//!
//! Kept apart from [`crate::samples`] so that the shapes worth
//! measuring on and the machinery that draws them can be read and
//! changed separately. What a corpus is made of is a decision; how a
//! bitmap is filled is a mechanism.

use crate::runmax::range_mask;
use crate::{BitMatrix, HEIGHT, WIDTH};

/// A bitmap grown from a seed, confined to a `side` by `side` corner.
///
/// The seed settles it entirely: the same four arguments give the same
/// bitmap on every run and every machine.
///
/// `density` is the share of the `side * side` cells that end up set.
/// `cluster` is how often a new cell lands beside one already set
/// rather than anywhere at all: at 0 the cells are scattered and every
/// one of them is its own rectangle; at 1 they only ever extend what is
/// already standing, so the bitmap comes out as a few solid blobs.
/// Everything interesting is in between.
///
/// Cells beside the ones already set are kept **with repeats**, so a
/// cell with three set neighbours is three times as likely to be taken
/// as one with a single neighbour. That is the point: it is what makes
/// a blob fill in rather than sprawl.
pub(super) fn one(seed: u64, side: usize, density: f64, cluster: f64) -> BitMatrix {
    let side = side.clamp(1, WIDTH.min(HEIGHT));
    let wanted = (density.clamp(0.0, 1.0) * (side * side) as f64) as usize;
    let cluster = (cluster.clamp(0.0, 1.0) * u32::MAX as f64) as u64;

    let mut bits = BitMatrix::new();
    // Xorshift needs a state that is not zero, and it is the seed alone
    // that has to reproduce the bitmap.
    let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    // Unset cells beside a set one, with repeats.
    let mut edge: Vec<(u8, u8)> = Vec::new();
    let mut standing = 0;

    while standing < wanted {
        let beside = (!edge.is_empty() && next() % (1 << 32) < cluster)
            .then(|| {
                while let Some(at) = (!edge.is_empty()).then(|| next() as usize % edge.len()) {
                    let cell = edge.swap_remove(at);
                    if !bits.get(cell.0, cell.1) {
                        return Some(cell);
                    }
                }
                None
            })
            .flatten();

        let (x, y) = match beside {
            Some(cell) => cell,
            None => anywhere_clear(&bits, side, &mut next),
        };

        bits.set(x, y);
        standing += 1;
        let (x, y) = (x as i32, y as i32);
        for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            if (0..side as i32).contains(&nx) && (0..side as i32).contains(&ny) {
                let (nx, ny) = (nx as u8, ny as u8);
                if !bits.get(nx, ny) {
                    edge.push((nx, ny));
                }
            }
        }
    }

    bits
}

/// Any cell of the corner still clear, found by guessing and then, once
/// guessing stops paying, by looking.
///
/// Guessing answers nearly every draw, because a bitmap is usually far
/// from full. The scan is there so that a density close to 1 still
/// finishes rather than rolling dice forever.
fn anywhere_clear(bits: &BitMatrix, side: usize, next: &mut impl FnMut() -> u64) -> (u8, u8) {
    for _ in 0..64 {
        let roll = next();
        let (x, y) = ((roll as usize % side) as u8, ((roll >> 32) as usize % side) as u8);
        if !bits.get(x, y) {
            return (x, y);
        }
    }

    // A word at a time, so that a nearly full corner is scanned in a
    // few hundred tests rather than tens of thousands. `side` is a
    // `usize` throughout: it can be 256, which a `u8` cannot hold, and
    // casting it early is how this went wrong once already.
    for y in 0..side {
        let row = bits.row(y as u8);
        for (index, &word) in row.iter().enumerate() {
            let inside = range_mask(index, 0, (side - 1) as u8);
            let clear = !word & inside;
            if clear != 0 {
                let x = index * 64 + clear.trailing_zeros() as usize;
                return (x as u8, y as u8);
            }
        }
    }
    panic!("a corner that is not already full");
}
