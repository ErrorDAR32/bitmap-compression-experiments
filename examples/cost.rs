//! The two metrics, both counted in instructions.
//!
//! Wall clock on a shared machine drifts by more than the differences
//! worth measuring, and it says nothing about a bitmap with twice the
//! content in it. Counting instructions fixes both: the count is the
//! same every run, and it divides by whatever you like.
//!
//! - **Instructions per set cell** is what an algorithm spends on the
//!   content of a bitmap rather than on the bitmap. It compares across
//!   bitmaps holding wildly different amounts.
//! - **Instruction-optimal bias** is the instructions taken multiplied
//!   by how many rectangles were given over the fewest possible. An
//!   algorithm can lose by being slow or by being wasteful and the two
//!   trade against each other, so neither alone says which is better.
//!   The accurate algorithm's bias is its instructions alone, since it
//!   is never over the fewest.
//!
//! Counting is callgrind's job, so this drives it. Each shape is run
//! three times -- building the bitmaps, building and partitioning them,
//! and building and solving them exactly -- and the differences are the
//! two algorithms alone. Building a sample is not free and has no
//! business in either figure.
//!
//! Run it with no arguments, or with a seed to start from. The same
//! seed gives the same bitmaps, so two runs are comparable down to the
//! instruction; a fresh seed asks whether what the last one showed was
//! about the algorithms or about those bitmaps. It needs `valgrind` on
//! the path.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{accurate, samples, RunmaxClipnmerge};
use std::process::Command;

/// How many bitmaps a shape is measured over. Enough to average, few
/// enough that both algorithms finish under valgrind: a dense ragged
/// bitmap runs to thousands of rectangles and costs a thousand times
/// what a sparse one does.
const EACH: u64 = 4;

/// What a child run is asked to do.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Doing {
    Building,
    Partitioning,
    Solving,
}

impl Doing {
    fn word(self) -> &'static str {
        match self {
            Doing::Building => "build",
            Doing::Partitioning => "partition",
            Doing::Solving => "solve",
        }
    }
}

/// One run. Prints the set cells and the rectangles, so the driver has
/// its denominator and its ratio.
fn run(density: f64, cluster: f64, from: u64, doing: Doing) {
    let maps: Vec<_> = samples::grown(from, density, cluster, EACH).collect();
    let cells: u32 = maps.iter().map(|b| b.count_set()).sum();
    let rects: usize = match doing {
        Doing::Building => 0,
        Doing::Partitioning => {
            let mut work = RunmaxClipnmerge::new();
            maps.iter().map(|bits| work.partition(bits).len()).sum()
        }
        Doing::Solving => maps.iter().map(|bits| accurate::partition(bits).len()).sum(),
    };
    println!("{cells} {rects}");
}

/// Runs one pass under callgrind and answers its instructions, set
/// cells and rectangles.
fn count(density: f64, cluster: f64, from: u64, doing: Doing) -> Option<(u64, u64, u64)> {
    let me = std::env::current_exe().ok()?;
    let out = Command::new("valgrind")
        .args(["--tool=callgrind", "--callgrind-out-file=/dev/null"])
        .arg(&me)
        .args(["run", &density.to_string(), &cluster.to_string(), &from.to_string(), doing.word()])
        .output()
        .ok()?;

    // Callgrind writes its total to stderr as "I   refs: 1,234,567".
    let stderr = String::from_utf8_lossy(&out.stderr);
    let refs = stderr.lines().find(|line| line.contains("I   refs:"))?;
    let took = refs.rsplit(':').next()?.trim().replace(',', "").parse().ok()?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut fields = stdout.split_whitespace();
    let cells = fields.next()?.parse().ok()?;
    let rects = fields.next()?.parse().ok()?;
    Some((took, cells, rects))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    if first.as_deref() == Some("run") {
        let mut number = || args.next().expect("a number").parse::<f64>().expect("a number");
        let (density, cluster, from) = (number(), number(), number() as u64);
        let doing = match args.next().as_deref() {
            Some("partition") => Doing::Partitioning,
            Some("solve") => Doing::Solving,
            _ => Doing::Building,
        };
        run(density, cluster, from, doing);
        return;
    }

    let from: u64 = first.and_then(|arg| arg.parse().ok()).unwrap_or(corpus::SEED);
    println!("counted under callgrind, the sample build taken out, seeds from {from}:");
    println!(
        "  {:<20} {:>8} {:>7} {:>8} {:>10} {:>12} {:>12}",
        "", "cells", "rects", "fewest", "per cell", "bias", "accurate's"
    );

    for shape in corpus::SHAPES {
        let (name, density, cluster) = (shape.name, shape.density, shape.cluster);
        let counted = [Doing::Building, Doing::Partitioning, Doing::Solving]
            .map(|doing| count(density, cluster, from, doing));
        let [Some((bare, cells, _)), Some((mesh, _, rects)), Some((solved, _, fewest))] = counted
        else {
            println!("  {name:<20}   (could not run valgrind)");
            continue;
        };

        let ours = mesh.saturating_sub(bare);
        let theirs = solved.saturating_sub(bare);
        let over = rects as f64 / fewest.max(1) as f64;
        println!(
            "  {name:<20} {cells:>8} {rects:>7} {fewest:>8} {:>10.1} {:>12.0} {:>12.0}",
            ours as f64 / cells.max(1) as f64,
            ours as f64 * over,
            theirs as f64,
        );
    }
}
