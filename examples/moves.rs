//! What each move of the clip-and-merge pass is worth against what it
//! costs, on the target metric.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{exact, Far, RunmaxClipnmerge};
use std::time::{Duration, Instant};

fn main() {
    let maps = corpus::realistic(2000);
    let minimum: usize = maps.iter().map(|b| exact::partition(b).len()).sum();
    let each = maps.len() as u32;

    println!("{} realistic bitmaps, best of 5, per bitmap:", maps.len());
    println!("  {:<24} {:>8} {:>10} {:>14}", "stopping after", "rects", "time", "target metric");

    for (label, far) in [
        ("the mesh", None),
        ("growing", Some(Far::Growing)),
        ("dissolving", Some(Far::Dissolving)),
        ("trimming", Some(Far::Trimming)),
    ] {
        let mut rects = 0;
        let mut fastest = Duration::MAX;
        for _ in 0..5 {
            let start = Instant::now();
            rects = 0;
            for bits in &maps {
                let mut mesh = RunmaxClipnmerge::from_bit_matrix(bits);
                if let Some(far) = far {
                    mesh.compact_to(far);
                }
                rects += std::hint::black_box(mesh.rects().len());
            }
            fastest = fastest.min(start.elapsed());
        }
        let per = fastest / each;
        let over = rects as f64 / minimum as f64;
        println!(
            "  {label:<24} {:>8.2} {:>10.1?} {:>14.1}",
            rects as f64 / each as f64,
            per,
            per.as_secs_f64() * 1e6 * over
        );
    }
    println!("  {:<24} {:>8.2}", "the minimum", minimum as f64 / each as f64);
}
