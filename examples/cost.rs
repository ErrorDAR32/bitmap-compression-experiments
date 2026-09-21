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
//! - **Instruction-optimal bias** is the instructions an optimal area
//!   costs, multiplied by `w.pow(w)` for `w` one more than the areas a
//!   bitmap is given over the fewest possible.
//!
//!   Every term is per bitmap. A run is [`EACH`] of them, and counting
//!   the whole run would put the waste of four bitmaps in an exponent
//!   that a single bitmap's waste belongs in, so a longer run would
//!   price the same algorithm worse. Dividing first makes the number
//!   mean something about one bitmap, and makes two runs of different
//!   lengths comparable.
//!
//!   The instructions are then divided again by the fewest areas, so
//!   that the cost is what an algorithm spends per area it had to
//!   produce rather than what it spends on a whole bitmap. That keeps
//!   the term at a few thousand instead of a few hundred million, and
//!   stops the size of the content leaking into a figure about waste.
//!
//!   One more than the waste, so that a partition never over the fewest
//!   is priced at its instructions and nothing is raised to the zeroth
//!   power. Counting the waste rather than the ratio is deliberate: an
//!   answer within 1% of the minimum is within 1% by the ratio however
//!   many areas it wastes, which made the metric read as instructions
//!   alone and say nothing about the waste.
//!
//!   It is still reported as a power of ten, because 180 areas wasted a
//!   bitmap prices at `10^410` and no float holds that. Read what it
//!   means before reading the numbers: the instruction term reaches
//!   `10^4` and the waste term `10^400`, so instructions are not a
//!   tie-break here, they are nothing at all. It is a waste metric with
//!   an instruction count attached, and its ranking is the ranking of
//!   `w` alone unless two answers waste the same. An
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
    let areas: usize = match doing {
        Doing::Building => 0,
        Doing::Partitioning => {
            let mut work = RunmaxClipnmerge::new();
            maps.iter().map(|bits| work.partition(bits).len()).sum()
        }
        Doing::Solving => maps.iter().map(|bits| accurate::partition(bits).len()).sum(),
    };
    println!("{cells} {areas}");
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
    let areas = fields.next()?.parse().ok()?;
    Some((took, cells, areas))
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

    println!(
        "\ninstruction-optimal bias, instructions an optimal area costs by w.pow(w),\n\
         for w one more than the areas a bitmap wastes, as a power of ten, same run:\n"
    );
    println!("{}", row(["shape", "runmax-clipnmerge", "accurate", "apart by", "winner"]));
    for &(name, _, areas, fewest, ours, theirs) in &measured {
        let (ours, theirs) = (log_bias(ours, areas, fewest), log_bias(theirs, fewest, fewest));
        println!(
            "{}",
            row([
                name,
                &format!("10^{ours:.1}"),
                &format!("10^{theirs:.1}"),
                &format!("10^{:.1}", (ours - theirs).abs()),
                if ours < theirs { "runmax wins" } else { "accurate wins" },
            ])
        );
    }
}

/// The base ten logarithm of the bias, since the bias itself does not
/// fit in anything. `w.pow(w)` in logarithms is `w * log(w)`, which is
/// why the metric can be reported at all.
///
/// The counts handed in are for a whole run of [`EACH`] bitmaps, so the
/// waste is divided by that to get what one bitmap wastes. The
/// instructions are not, because dividing them by the run's total
/// `fewest` already does it: both are sums over the same bitmaps, so
/// the run length cancels and what is left is the instructions an
/// optimal area costs.
fn log_bias(instructions: u64, areas: u64, fewest: u64) -> f64 {
    let per_area = instructions.max(1) as f64 / fewest.max(1) as f64;
    let w = areas.saturating_sub(fewest) as f64 / EACH as f64 + 1.0;
    per_area.log10() + w * w.log10()
}
