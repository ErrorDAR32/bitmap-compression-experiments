//! Halving against the other two: how many areas, how fast, and how
//! many bits it takes to write the bitmap down.
//!
//! The third column is the one the other algorithms cannot answer.
//! Runmax and the minimum partition produce a list of areas and nothing
//! else; storing one costs four bytes an area, and the list says
//! nothing about where anything is without reading all of it. Halving
//! produces a tree, which costs two bit sequences and answers "what is
//! in this quarter" by walking one branch.
//!
//! So the comparison is not apples to apples on purpose. Areas and time
//! say what it costs as a partitioner; bits say what it is for.

use bitmatrix::{accurate, samples, Halving, RunmaxClipnmerge};
use std::time::{Duration, Instant};

/// How many times each shape is timed, best taken.
const REPEATS: usize = 5;

/// One line of the report, header and data alike, so a column cannot be
/// labelled at one width and filled at another.
fn row(fields: [&str; 7]) -> String {
    const WIDTHS: [usize; 7] = [20, 9, 9, 9, 10, 10, 10];
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
            let started = Instant::now();
            run();
            started.elapsed()
        })
        .min()
        .expect("REPEATS is not zero")
}

fn main() {
    println!(
        "halving against the other two, best of {REPEATS}, seeds from {}:\n",
        samples::SAMPLE_SEED
    );
    println!(
        "{}",
        row(["shape", "fewest", "runmax", "halving", "areas", "runmax", "halving"])
    );
    println!("{}", row(["", "areas", "areas", "areas", "vs fewest", "time", "time"]));

    let (mut all_fewest, mut all_runmax, mut all_halving) = (0usize, 0usize, 0usize);
    let mut sizes: Vec<(&str, usize, usize, usize)> = Vec::new();

    for shape in samples::SHAPES {
        let maps: Vec<_> = shape.timed().collect();
        let n = maps.len();

        let mut work = RunmaxClipnmerge::new();
        let runmax: usize = maps.iter().map(|b| work.partition(b).len()).sum();
        let fewest: usize = maps.iter().map(|b| accurate::partition(b).len()).sum();

        let mut tree = Halving::new();
        let (mut halving, mut bits) = (0usize, 0usize);
        for map in &maps {
            halving += tree.partition(map).len();
            bits += tree.encoded_bits();
        }

        // Both timed in the same run on the same machine, since a
        // figure carried over from another session is not a comparison.
        let theirs = best_of(|| {
            for map in &maps {
                std::hint::black_box(work.partition(map).len());
            }
        }) / n as u32;
        let took = best_of(|| {
            for map in &maps {
                std::hint::black_box(tree.partition(map).len());
            }
        }) / n as u32;

        all_fewest += fewest;
        all_runmax += runmax;
        all_halving += halving;

        println!(
            "{}",
            row([
                shape.name,
                &(fewest / n).to_string(),
                &(runmax / n).to_string(),
                &(halving / n).to_string(),
                &format!("{:.2}x", halving as f64 / fewest.max(1) as f64),
                &format!("{theirs:.1?}"),
                &format!("{took:.1?}"),
            ])
        );
        sizes.push((shape.name, bits / n, fewest / n, halving / n));
    }

    println!(
        "\n  over the whole corpus halving spends {:.2}x the fewest areas, runmax {:.3}x.",
        all_halving as f64 / all_fewest as f64,
        all_runmax as f64 / all_fewest as f64
    );

    // What it costs to write the bitmap down three ways. An area is
    // four bytes, and the bitmap itself is always 65536 bits.
    println!("\nwriting one bitmap down, in bits:\n");
    println!(
        "{}",
        row(["shape", "raw bits", "fewest", "halving", "vs raw", "vs fewest", ""])
    );
    for (name, bits, fewest, _) in &sizes {
        let listed = fewest * 32;
        println!(
            "{}",
            row([
                name,
                "65536",
                &listed.to_string(),
                &bits.to_string(),
                &format!("{:.2}x", *bits as f64 / 65536.0),
                &format!("{:.2}x", *bits as f64 / listed as f64),
                "",
            ])
        );
    }
}
