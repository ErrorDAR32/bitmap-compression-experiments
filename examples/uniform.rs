//! What a bitmap drawn uniformly from all 2^65536 actually looks like.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{exact, reflex_corners, BitMatrix, Fastile};
use corpus::Sequence;
use std::time::Instant;

fn uniform(seq: &mut Sequence) -> BitMatrix {
    let mut bits = BitMatrix::new();
    // Every cell set or not on its own coin toss, which is what drawing
    // uniformly from all of them means.
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if seq.step().is_multiple_of(2) {
                bits.set(x, y);
            }
        }
    }
    bits
}

fn main() {
    let mut seq = Sequence::from(0x9E3779B97F4A7C15);
    let maps: Vec<_> = (0..20).map(|_| uniform(&mut seq)).collect();

    let cells: u64 = maps.iter().map(|b| b.count_set() as u64).sum();
    let corners: u64 = maps.iter().map(|b| reflex_corners(b).len() as u64).sum();
    let runs: u64 = maps.iter().map(|b| Fastile::count_runs(b) as u64).sum();
    let n = maps.len() as u64;

    println!("{n} bitmaps drawn uniformly, per bitmap:");
    println!("  {:>7} set cells of 65536", cells / n);
    println!("  {:>7} runs in the two lists", runs / n);
    println!("  {:>7} reflex corners", corners / n);

    let start = Instant::now();
    let best: usize = maps.iter().map(|b| exact::partition(b).len()).sum();
    let exact_time = start.elapsed() / maps.len() as u32;

    let start = Instant::now();
    let mut got = 0;
    for bits in &maps {
        let mut mesh = Fastile::from_bit_matrix(bits);
        mesh.compact();
        corpus::assert_partition(bits, mesh.rects(), "uniform");
        got += mesh.rects().len();
    }
    let fastile_time = start.elapsed() / maps.len() as u32;

    println!(
        "\n  exact    {:>6} rectangles in {exact_time:.1?}",
        best as u64 / n
    );
    println!(
        "  fastile  {:>6} rectangles in {fastile_time:.1?}  ({:.2}% over)",
        got as u64 / n,
        100.0 * (got as f64 / best as f64 - 1.0)
    );
    println!(
        "\n  four bytes a rectangle is {} bytes against 8192 for the raw bitmap: {:.1}x",
        best as u64 / n * 4,
        (best as f64 / n as f64 * 4.0) / 8192.0
    );

    // For contrast, the corpus the algorithm was tuned on.
    let real = corpus::realistic(20);
    let real_cells: u64 = real.iter().map(|b| b.count_set() as u64).sum();
    let real_best: usize = real.iter().map(|b| exact::partition(b).len()).sum();
    println!(
        "\n  a realistic bitmap: {} set cells, {} rectangles, {:.2}x the raw size",
        real_cells / 20,
        real_best as u64 / 20,
        (real_best as f64 / 20.0 * 4.0) / 8192.0
    );
}
