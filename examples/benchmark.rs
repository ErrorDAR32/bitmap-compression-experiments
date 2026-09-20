//! The greedy mesher against the minimum partition, on the whole corpus.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{optimal, BitMatrix, RunMesh};
use std::time::{Duration, Instant};

fn greedy(bits: &BitMatrix) -> (usize, Duration) {
    let start = Instant::now();
    let mut mesh = RunMesh::from_bit_matrix(bits);
    mesh.compact();
    let took = start.elapsed();
    corpus::assert_partition(bits, mesh.rects(), "greedy");
    (mesh.rects().len(), took)
}

fn minimum(bits: &BitMatrix) -> (usize, Duration) {
    let start = Instant::now();
    let rects = optimal::partition(bits);
    let took = start.elapsed();
    corpus::assert_partition(bits, &rects, "minimum");
    (rects.len(), took)
}

fn line(label: &str, bits: &BitMatrix) {
    let (got, greedy_time) = greedy(bits);
    let (best, exact_time) = minimum(bits);
    println!(
        "  {label:<22} {got:>6} in {greedy_time:>8.1?}   minimum {best:>6} in {exact_time:>8.1?}   {:.2}x the rectangles",
        got as f64 / best.max(1) as f64
    );
}

fn main() {
    let maps = corpus::realistic(1000);
    let (mut greedy_time, mut exact_time) = (Duration::ZERO, Duration::ZERO);
    let (mut got, mut best) = (0usize, 0usize);
    let (mut worst_greedy, mut worst_exact) = (Duration::ZERO, Duration::ZERO);

    for bits in &maps {
        let (n, took) = greedy(bits);
        got += n;
        greedy_time += took;
        worst_greedy = worst_greedy.max(took);

        let (n, took) = minimum(bits);
        best += n;
        exact_time += took;
        worst_exact = worst_exact.max(took);
    }

    let n = maps.len() as u32;
    println!("{n} realistic bitmaps, per bitmap:");
    println!(
        "  greedy   {:>8.2} rects  {:>9.1?}  worst {:>9.1?}",
        got as f64 / n as f64,
        greedy_time / n,
        worst_greedy
    );
    println!(
        "  minimum  {:>8.2} rects  {:>9.1?}  worst {:>9.1?}",
        best as f64 / n as f64,
        exact_time / n,
        worst_exact
    );
    println!(
        "  greedy is {:.2}% over the minimum, in {:.2}x the time\n",
        100.0 * (got as f64 / best as f64 - 1.0),
        greedy_time.as_secs_f64() / exact_time.as_secs_f64()
    );

    println!("the hard cases:");
    line("checkerboard", &corpus::checkerboard());
    for (name, rows) in corpus::WORST {
        line(name, &corpus::tiled(rows));
    }
    let mut full = BitMatrix::new();
    full.set_rect(0, 0, 255, 255);
    line("solid square", &full);
}
