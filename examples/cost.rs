//! The two metrics, both counted in instructions.
//!
//! Wall clock on a shared machine drifts by more than the differences
//! worth measuring, and it says nothing about a bitmap with twice the
//! content in it. Counting instructions fixes both: the count is the
//! same every run, and it divides by whatever you like.
//!
//! - **Instructions per set cell** is what the algorithm spends on the
//!   content of a bitmap rather than on the bitmap. It compares across
//!   bitmaps holding wildly different amounts.
//! - **Instruction-optimal bias** is the instructions taken multiplied
//!   by how many rectangles were given over the fewest possible. An
//!   algorithm can lose by being slow or by being wasteful and the two
//!   trade against each other, so neither alone says which is better.
//!   The exact algorithm's bias is its instructions alone, since it is
//!   never over the fewest.
//!
//! Counting is callgrind's job, so this drives it. Each case is run
//! three times -- building the bitmaps, building and partitioning them,
//! and building and solving them exactly -- and the differences are the
//! two algorithms alone. Building a corpus is not free and has no
//! business in either figure.
//!
//! Run it with no arguments, or with a seed to start the grown bitmaps
//! from. The same seed gives the same bitmaps, so two runs of this are
//! comparable down to the instruction; a fresh seed asks whether what
//! the last one showed was about the algorithm or about those bitmaps.
//! It needs `valgrind` on the path.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{exact, BitMatrix, RunmaxClipnmerge};
use std::process::Command;

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

/// The cases, named by the argument that builds them. A grown case is
/// `grown:<density>:<cluster>:<seed>`, carrying the seed so that the
/// child runs build the same bitmaps the parent asked for.
fn cases(from: u64) -> Vec<String> {
    let mut named: Vec<String> = Vec::new();
    for density in ["0.05", "0.20", "0.50"] {
        for cluster in ["0.00", "0.70", "0.95"] {
            named.push(format!("grown:{density}:{cluster}:{from}"));
        }
    }
    named.push("realistic".into());
    named.push("solid".into());
    for (motif, _) in corpus::WORST {
        named.push(motif.into());
    }
    named
}

/// How many bitmaps a grown case holds. Enough to average over, few
/// enough that both algorithms finish under valgrind: a dense clustered
/// bitmap runs to thousands of rectangles and costs a thousand times
/// what a realistic one does.
const GROWN: u64 = 4;

fn build(case: &str) -> Vec<BitMatrix> {
    if let Some(rest) = case.strip_prefix("grown:") {
        let mut fields = rest.split(':');
        let density: f64 = fields.next().expect("a density").parse().expect("a density");
        let cluster: f64 = fields.next().expect("a cluster").parse().expect("a cluster");
        let from: u64 = fields.next().expect("a seed").parse().expect("a seed");
        return (from..from + GROWN).map(|s| BitMatrix::grown(s, density, cluster)).collect();
    }
    match case {
        "realistic" => corpus::realistic(50),
        "solid" => {
            let mut bits = BitMatrix::new();
            bits.set_rect(0, 0, 255, 255);
            vec![bits]
        }
        name => {
            let motif = corpus::WORST
                .iter()
                .find(|(known, _)| *known == name)
                .map(|(_, rows)| *rows)
                .expect("a known motif");
            vec![corpus::tiled(motif)]
        }
    }
}

/// One run. Prints the set cells and the rectangles, so the driver has
/// its denominator and its ratio.
fn run(case: &str, doing: Doing) {
    let maps = build(case);
    let cells: u32 = maps.iter().map(|b| b.count_set()).sum();
    let rects: usize = match doing {
        Doing::Building => 0,
        Doing::Partitioning => {
            let mut work = RunmaxClipnmerge::new();
            maps.iter().map(|bits| work.partition(bits).len()).sum()
        }
        Doing::Solving => maps.iter().map(|bits| exact::partition(bits).len()).sum(),
    };
    println!("{cells} {rects}");
}

/// Runs one pass under callgrind and answers its instructions, set
/// cells and rectangles.
fn count(case: &str, doing: Doing) -> Option<(u64, u64, u64)> {
    let me = std::env::current_exe().ok()?;
    let out = Command::new("valgrind")
        .args(["--tool=callgrind", "--callgrind-out-file=/dev/null"])
        .arg(&me)
        .args(["run", case, doing.word()])
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
        let case = args.next().expect("a case to run");
        let doing = match args.next().as_deref() {
            Some("partition") => Doing::Partitioning,
            Some("solve") => Doing::Solving,
            _ => Doing::Building,
        };
        run(&case, doing);
        return;
    }

    let from: u64 = first.and_then(|arg| arg.parse().ok()).unwrap_or(0);
    println!("counted under callgrind, the corpus build taken out, seeds from {from}:");
    println!(
        "  {:<24} {:>8} {:>7} {:>8} {:>10} {:>12} {:>12}",
        "", "cells", "rects", "fewest", "per cell", "bias", "exact's bias"
    );

    for case in cases(from) {
        let counted = [Doing::Building, Doing::Partitioning, Doing::Solving]
            .map(|doing| count(&case, doing));
        let [Some((bare, cells, _)), Some((mesh, _, rects)), Some((solved, _, fewest))] = counted
        else {
            println!("  {case:<24}   (could not run valgrind)");
            continue;
        };

        let ours = mesh.saturating_sub(bare);
        let theirs = solved.saturating_sub(bare);
        let over = rects as f64 / fewest.max(1) as f64;
        println!(
            "  {case:<24} {cells:>8} {rects:>7} {fewest:>8} {:>10.1} {:>12.0} {:>12.0}",
            ours as f64 / cells.max(1) as f64,
            ours as f64 * over,
            theirs as f64,
        );
    }
}
