//! What the first pass costs: splitting off the cells that stand alone,
//! and reducing what is left to run lists.
//!
//! Both are done before a single rectangle is decided, so whatever they
//! cost is a floor under the whole algorithm.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{reflex_corners, BitMatrix, Fastile};
use std::time::{Duration, Instant};

const BITMAPS: usize = 10_000;
const REPEATS: usize = 5;

/// The best of several sweeps, since interference can only ever make one
/// slower than the machine was capable of.
fn best(maps: &[BitMatrix], mut sweep: impl FnMut(&BitMatrix) -> usize) -> (usize, Duration) {
    let mut total = 0;
    let mut fastest = Duration::MAX;
    for _ in 0..REPEATS {
        let start = Instant::now();
        total = 0;
        for bits in maps {
            total += std::hint::black_box(sweep(bits));
        }
        fastest = fastest.min(start.elapsed());
    }
    (total, fastest)
}

fn main() {
    let maps = corpus::realistic(BITMAPS);
    let n = maps.len() as u32;
    let cells: u64 = maps.iter().map(|b| b.count_set() as u64).sum();

    // Warm up.
    for bits in &maps {
        std::hint::black_box(Fastile::count_runs(bits));
    }

    let (alone, split_time) = best(&maps, |bits| bits.split_isolated().0.count_set() as usize);
    let (runs, runs_time) = best(&maps, |bits| Fastile::count_runs(&bits.split_isolated().1));
    let (_, whole_time) = best(&maps, |bits| {
        let mut mesh = Fastile::from_bit_matrix(bits);
        mesh.compact();
        mesh.rects().len()
    });

    // The run building on its own, with the split already paid for.
    let split: Vec<BitMatrix> = maps.iter().map(|b| b.split_isolated().1).collect();
    let (_, only_runs) = best(&split, Fastile::count_runs);

    let (corners, corner_time) = best(&maps, |bits| reflex_corners(bits).len());

    println!("{n} realistic bitmaps, best of {REPEATS}, per bitmap:");
    println!("  {:>8} set cells", cells / n as u64);
    println!("  {:>8} cells standing alone", alone as u64 / n as u64);
    println!("  {:>8} runs in the two lists", runs as u64 / n as u64);
    println!("  {:>8} reflex corners", corners as u64 / n as u64);
    println!();
    println!("  splitting off the cells alone   {:>9.1?}", split_time / n);
    println!("  building the run lists          {:>9.1?}", only_runs / n);
    println!("  finding the reflex corners      {:>9.1?}", corner_time / n);
    println!("  both together                   {:>9.1?}", runs_time / n);
    println!("  the whole thing, meshed and compacted  {:>9.1?}", whole_time / n);
    println!(
        "\n  the first pass is {:.0}% of the whole run",
        100.0 * runs_time.as_secs_f64() / whole_time.as_secs_f64()
    );

    // A bitmap with nothing in it, to show the pass costs the same
    // whatever it is given.
    let empty = vec![BitMatrix::new(); 1000];
    let (_, empty_time) = best(&empty, Fastile::count_runs);
    let solid: Vec<BitMatrix> = (0..1000)
        .map(|_| {
            let mut b = BitMatrix::new();
            b.set_rect(0, 0, 255, 255);
            b
        })
        .collect();
    let (_, solid_time) = best(&solid, Fastile::count_runs);
    println!(
        "  on an empty bitmap {:.1?}, on a full one {:.1?}",
        empty_time / 1000,
        solid_time / 1000
    );
}
