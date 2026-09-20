//! The one source of test bitmaps.
//!
//! Nothing in this crate is measured on a bitmap anybody drew by hand.
//! Every sample comes from here, so changing what the tests and the
//! examples run on is a change in one place.
//!
//! A sample is settled entirely by three numbers, which is what makes a
//! result reproducible: the same seed, density and cluster weight give
//! the same bitmap on every run and every machine. Two runs of a
//! benchmark are therefore comparable down to the instruction, and a
//! fresh seed range asks whether what the last one showed was about the
//! algorithm or about those particular bitmaps.
//!
//! Hand-drawn shapes used to live here -- unions of circles, a
//! checkerboard, three tiled motifs found by search. They are gone, and
//! little was lost: the generator reaches worse ground than any of
//! them. The worst motif cost 13,057 instructions per set cell, where
//! `grown(_, 0.50, 0.00)` costs 124,610.

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
fn one(seed: u64, side: usize, density: f64, cluster: f64) -> BitMatrix {
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

/// A run of bitmaps from consecutive seeds, built one at a time.
///
/// Lazy because a caller measuring two thousand of them has no reason
/// to hold two thousand at once, and because the sweeps that compare
/// seed ranges would otherwise spend their first seconds allocating.
pub struct Samples {
    seed: u64,
    left: u64,
    side: usize,
    density: f64,
    cluster: f64,
}

/// `count` bitmaps from seeds `seed`, `seed + 1`, and so on.
///
/// `density` is the share of the 65536 cells set in each. `cluster` is
/// how often a new cell lands beside one already set rather than
/// anywhere at all: at 0 the cells are scattered and every one is its
/// own rectangle, at 1 they only extend what is standing and the
/// bitmap is a few solid blobs. Between them the two parameters cover
/// the cases that used to be drawn by hand.
pub fn grown(seed: u64, density: f64, cluster: f64, count: u64) -> Samples {
    grown_in(seed, WIDTH, density, cluster, count)
}

/// The same, confined to a `side` by `side` corner of the matrix.
///
/// For callers that cannot afford a full one: exhaustive search is
/// exponential in the cells, so the ground truth runs on corners of
/// eight or fewer.
pub fn grown_in(seed: u64, side: usize, density: f64, cluster: f64, count: u64) -> Samples {
    Samples { seed, left: count, side, density, cluster }
}

/// One bitmap, for a caller that wants a single sample rather than a
/// run of them.
pub fn one_grown(seed: u64, density: f64, cluster: f64) -> BitMatrix {
    one(seed, WIDTH, density, cluster)
}

impl Iterator for Samples {
    type Item = BitMatrix;

    fn next(&mut self) -> Option<BitMatrix> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        let seed = self.seed;
        self.seed += 1;
        Some(one(seed, self.side, self.density, self.cluster))
    }
}

impl ExactSizeIterator for Samples {
    fn len(&self) -> usize {
        self.left as usize
    }
}

/// Panics unless the rectangles cover exactly the set bits, once each.
///
/// Overlap falls out of arithmetic rather than comparing every pair: if
/// the areas sum to more than the cells painted, two rectangles covered
/// the same cell.
pub fn assert_partition(bits: &BitMatrix, rects: &[crate::Rect], label: &str) {
    let mut painted = BitMatrix::new();
    for r in rects {
        painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
    }
    assert_eq!(painted.count_set(), bits.count_set(), "{label}: wrong coverage");
    let area: u32 = rects.iter().map(|r| r.area()).sum();
    assert_eq!(area, painted.count_set(), "{label}: rectangles overlap");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same seed has to give the same bitmap, and a different one a
    /// different bitmap, or a reproduction is not a reproduction.
    #[test]
    fn growing_is_settled_by_its_seed() {
        for (density, cluster) in [(0.05, 0.0), (0.2, 0.5), (0.4, 0.9), (0.9, 0.95)] {
            let once = one_grown(12345, density, cluster);
            let again = one_grown(12345, density, cluster);
            assert_eq!(once.words, again.words, "the same seed wandered");

            let other = one_grown(12346, density, cluster);
            assert_ne!(once.words, other.words, "two seeds agreed");

            let wanted = (density * 65536.0) as u32;
            assert_eq!(once.count_set(), wanted, "the density is not the density");
        }
    }

    /// Clustering has to do what it says: at nothing the set cells are
    /// scattered and hardly any of them touch, at almost everything
    /// they are in blobs and nearly all of them do.
    #[test]
    fn clustering_decides_how_much_touches() {
        let touching = |bits: &BitMatrix| {
            let mut with = 0;
            for y in 0..=u8::MAX {
                for x in 0..=u8::MAX {
                    if bits.get(x, y) {
                        let (x, y) = (x as i32, y as i32);
                        let near = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                            .into_iter()
                            .filter(|(nx, ny)| (0..256).contains(nx) && (0..256).contains(ny))
                            .any(|(nx, ny)| bits.get(nx as u8, ny as u8));
                        with += u32::from(near);
                    }
                }
            }
            with as f64 / bits.count_set() as f64
        };

        let scattered = touching(&one_grown(7, 0.1, 0.0));
        let blobs = touching(&one_grown(7, 0.1, 0.99));
        assert!(scattered < 0.45, "scattered cells touch too much: {scattered}");
        assert!(blobs > 0.95, "clustered cells touch too little: {blobs}");
        assert!(blobs > scattered * 2.0, "clustering made no difference");
    }
}
