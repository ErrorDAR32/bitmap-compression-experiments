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

use bitmatrix::{accurate, samples, RunmaxClipnmerge};
use std::process::Command;

/// How many bitmaps a shape is measured over. Enough to average, few
/// enough that both algorithms finish under valgrind: a dense ragged
/// bitmap runs to thousands of rectangles and costs a thousand times
/// what a sparse one does.
const EACH: u64 = 4;

/// One line of a report, header and data alike.
///
/// Both go through here, so a column cannot be labelled at one width
/// and filled at another. Doing it by hand is how the last report came
/// out crooked.
fn row(fields: [&str; 5]) -> String {
    const WIDTHS: [usize; 5] = [20, 18, 14, 12, 16];
    let mut out = String::from("  ");
    for (index, (field, width)) in fields.iter().zip(WIDTHS).enumerate() {
        if index > 0 {
            out.push(' ');
        }
        // The first column reads as a label, the rest as figures.
        if index == 0 {
            out.push_str(&format!("{field:<width$}"));
        } else {
            out.push_str(&format!("{field:>width$}"));
        }
    }
    out.trim_end().to_string()
}

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

    let from: u64 = first.and_then(|arg| arg.parse().ok()).unwrap_or(samples::SAMPLE_SEED);

    let mut measured = Vec::new();
    for shape in samples::SHAPES {
        let counted = [Doing::Building, Doing::Partitioning, Doing::Solving]
            .map(|doing| count(shape.density, shape.cluster, from, doing));
        let [Some((bare, cells, _)), Some((mesh, _, areas)), Some((solved, _, fewest))] = counted
        else {
            println!("  {}   (could not run valgrind)", shape.name);
            continue;
        };
        measured.push((shape.name, cells, areas, fewest, mesh - bare, solved - bare));
    }

    println!(
        "instructions per active cell, counted under callgrind with the \
         sample build taken out, seeds from {from}:\n"
    );
    println!("{}", row(["shape", "active cells", "areas", "fewest", "per active cell"]));
    for &(name, cells, areas, fewest, ours, _) in &measured {
        println!(
            "{}",
            row([
                name,
                &cells.to_string(),
                &areas.to_string(),
                &fewest.to_string(),
                &format!("{:.1}", ours as f64 / cells.max(1) as f64),
            ])
        );
    }

    println!("\ninstruction-optimal bias, instructions by areas over fewest, same run:\n");
    println!("{}", row(["shape", "runmax-clipnmerge", "accurate", "ratio", "winner"]));
    for &(name, _, areas, fewest, ours, theirs) in &measured {
        let over = areas as f64 / fewest.max(1) as f64;
        let (ours, theirs) = (ours as f64 * over, theirs as f64);
        println!(
            "{}",
            row([
                name,
                &format!("{ours:.0}"),
                &format!("{theirs:.0}"),
                &format!("{:.2}x", ours / theirs),
                if ours < theirs { "runmax wins" } else { "accurate wins" },
            ])
        );
    }
}
