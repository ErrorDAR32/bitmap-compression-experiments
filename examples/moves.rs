//! What each move of the clip-and-merge pass is worth against what it
//! costs.
//!
//! Times rather than instructions, because the shape of the trade is
//! what matters here and it is the same either way. The metrics live in
//! the `cost` example.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{exact, BitMatrix, Far, RunmaxClipnmerge};
use std::time::{Duration, Instant};

fn main() {
    report("realistic", corpus::realistic(2000));
    // Grown bitmaps at the settings the metrics found worst: enough
    // content to be connected, ragged enough to be all boundary.
    for (density, cluster) in [(0.05, 0.70), (0.20, 0.70), (0.20, 0.95)] {
        let maps: Vec<BitMatrix> =
            (0..4).map(|seed| BitMatrix::grown(seed, density, cluster)).collect();
        report(&format!("grown {density:.2} {cluster:.2}"), maps);
    }
}

fn report(label: &str, maps: Vec<BitMatrix>) {
    let minimum: usize = maps.iter().map(|b| exact::partition(b).len()).sum();
    let each = maps.len() as u32;

    println!("\n{} {label} bitmaps, best of 5, per bitmap:", maps.len());
    println!("  {:<24} {:>8} {:>10} {:>14}", "stopping after", "rects", "time", "over the fewest");

    let mut work = RunmaxClipnmerge::new();
    for (label, far) in [
        ("the mesh", None),
        ("growing", Some(Far::Growing)),
        ("merging", Some(Far::Merging)),
        ("clipping", Some(Far::Clipping)),
    ] {
        let mut rects = 0;
        let mut fastest = Duration::MAX;
        for _ in 0..5 {
            let start = Instant::now();
            rects = 0;
            for bits in &maps {
                rects += std::hint::black_box(work.partition_to(bits, far).len());
            }
            fastest = fastest.min(start.elapsed());
        }
        let per = fastest / each;
        let over = rects as f64 / minimum as f64;
        println!(
            "  {label:<24} {:>8.2} {:>10.1?} {:>14.2}%",
            rects as f64 / each as f64,
            per,
            100.0 * (over - 1.0)
        );
    }
    println!("  {:<24} {:>8.2}", "the minimum", minimum as f64 / each as f64);
}
