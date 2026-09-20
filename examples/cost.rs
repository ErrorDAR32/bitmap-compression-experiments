//! Instructions per set cell: what the algorithm spends on the content
//! of a bitmap rather than on the bitmap.
//!
//! Wall clock on a shared machine drifts by more than the differences
//! worth measuring, and it says nothing about a bitmap with twice the
//! content in it. Counting instructions fixes both: the count is the
//! same every run, and dividing by the set cells says what a cell
//! costs, which is comparable across bitmaps that hold wildly different
//! amounts.
//!
//! Counting is callgrind's job, so this drives it. Each case is run
//! twice, once building the bitmaps and once building and partitioning
//! them, and the difference is the algorithm alone -- building a corpus
//! of circles and rectangles is not free and has no business in the
//! figure.
//!
//! Run it with no arguments. It needs `valgrind` on the path.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{BitMatrix, RunmaxClipnmerge};
use std::process::Command;

/// The cases, each named by the argument that builds it.
const CASES: [&str; 6] = [
    "realistic",
    "checkerboard",
    "solid",
    "spine and ribs",
    "spine and two ribs",
    "ladder",
];

fn build(case: &str) -> Vec<BitMatrix> {
    match case {
        "realistic" => corpus::realistic(50),
        "checkerboard" => vec![corpus::checkerboard()],
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

/// One run: builds the case, and partitions it unless told only to
/// build. Prints the set cells so the driver has its denominator.
fn work(case: &str, partition: bool) {
    let maps = build(case);
    let cells: u32 = maps.iter().map(|b| b.count_set()).sum();
    if partition {
        let mut work = RunmaxClipnmerge::new();
        let mut total = 0;
        for bits in &maps {
            total += work.partition(bits).len();
        }
        println!("cells {cells} rects {total}");
    } else {
        println!("cells {cells} rects 0");
    }
}

/// Runs one pass under callgrind and answers the instructions it took
/// and what it printed.
fn count(case: &str, partition: bool) -> Option<(u64, String)> {
    let me = std::env::current_exe().ok()?;
    let out = Command::new("valgrind")
        .args(["--tool=callgrind", "--callgrind-out-file=/dev/null"])
        .arg(&me)
        .arg("run")
        .arg(case)
        .arg(if partition { "full" } else { "build" })
        .output()
        .ok()?;

    // Callgrind writes its total to stderr as "I   refs: 1,234,567".
    let stderr = String::from_utf8_lossy(&out.stderr);
    let refs = stderr.lines().find(|line| line.contains("I   refs:"))?;
    let count = refs
        .rsplit(':')
        .next()?
        .trim()
        .replace(',', "")
        .parse::<u64>()
        .ok()?;
    Some((count, String::from_utf8_lossy(&out.stdout).trim().to_string()))
}

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("run") {
        let case = args.next().expect("a case to run");
        let full = args.next().as_deref() == Some("full");
        work(&case, full);
        return;
    }

    println!("instructions per set cell, counted under callgrind:");
    println!(
        "  {:<22} {:>10} {:>8} {:>14} {:>10}",
        "", "cells", "rects", "instructions", "per cell"
    );

    for case in CASES {
        let Some((bare, _)) = count(case, false) else {
            println!("  {case:<22}   (could not run valgrind)");
            continue;
        };
        let Some((full, printed)) = count(case, true) else {
            println!("  {case:<22}   (could not run valgrind)");
            continue;
        };

        let mut fields = printed.split_whitespace();
        let cells: u64 = fields.nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
        let rects: u64 = fields.nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
        let spent = full.saturating_sub(bare);

        println!(
            "  {case:<22} {cells:>10} {rects:>8} {spent:>14} {:>10.1}",
            spent as f64 / cells.max(1) as f64
        );
    }
}
