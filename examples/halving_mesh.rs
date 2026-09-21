//! The halving tree used as a mesh, against runmax's own.
//!
//! Both feed the same rewriting pass, which does not care where its
//! areas came from. The question is whether a mediocre mesh built very
//! cheaply beats a good mesh built dearly, once the passes have had
//! their way with both.

use bitmatrix::{accurate, samples, BitMatrix, Halving, RunmaxClipnmerge, Stop};
use std::time::{Duration, Instant};

const REPEATS: usize = 5;

fn row(fields: [&str; 7]) -> String {
    const WIDTHS: [usize; 7] = [20, 9, 9, 9, 9, 10, 10];
    let mut out = String::from("  ");
    for (index, (field, width)) in fields.iter().zip(WIDTHS).enumerate() {
        if index > 0 {
            out.push(' ');
        }
        if index == 0 {
            out.push_str(&format!("{field:<width$}"));
        } else {
            out.push_str(&format!("{field:>width$}"));
        }
    }
    out.trim_end().to_string()
}

fn best_of(mut run: impl FnMut()) -> Duration {
    (0..REPEATS)
        .map(|_| {
            let at = Instant::now();
            run();
            at.elapsed()
        })
        .min()
        .expect("REPEATS is not zero")
}

fn main() {
    println!("two meshes into the same rewriting pass, best of {REPEATS}:\n");
    println!(
        "{}",
        row(["shape", "fewest", "runmax", "halving", "halving", "runmax", "halving"])
    );
    println!(
        "{}",
        row(["", "areas", "areas", "meshed", "rewritten", "time", "time"])
    );

    let (mut af, mut ar, mut ah) = (0usize, 0usize, 0usize);

    for shape in samples::SHAPES {
        let maps: Vec<BitMatrix> = shape.timed().collect();
        let n = maps.len();

        let fewest: usize = maps.iter().map(|b| accurate::partition(b).len()).sum();
        let mut work = RunmaxClipnmerge::new();
        let runmax: usize = maps.iter().map(|b| work.partition(b).len()).sum();

        let mut tree = Halving::new();
        let meshed: usize = maps.iter().map(|b| tree.partition(b).len()).sum();
        let rewritten: usize = maps
            .iter()
            .map(|b| tree.partition_rewritten(b, Stop::AfterMerging).len())
            .sum();

        let theirs = best_of(|| {
            for b in &maps {
                std::hint::black_box(work.partition(b).len());
            }
        }) / n as u32;
        let ours = best_of(|| {
            for b in &maps {
                std::hint::black_box(tree.partition_rewritten(b, Stop::AfterMerging).len());
            }
        }) / n as u32;

        af += fewest;
        ar += runmax;
        ah += rewritten;

        println!(
            "{}",
            row([
                shape.name,
                &(fewest / n).to_string(),
                &(runmax / n).to_string(),
                &(meshed / n).to_string(),
                &(rewritten / n).to_string(),
                &format!("{theirs:.1?}"),
                &format!("{ours:.1?}"),
            ])
        );
    }

    println!(
        "\n  over the corpus: runmax {:.3}x the fewest, halving rewritten {:.3}x",
        ar as f64 / af as f64,
        ah as f64 / af as f64
    );
}
