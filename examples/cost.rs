//! The metric, counted in instructions.
//!
//! Wall clock on a shared machine drifts by more than the differences
//! worth measuring, and it says nothing about a bitmap with twice the
//! content in it. Counting instructions fixes both: the count is the
//! same every run, and it divides by whatever you like.
//!
//! - **Instructions per set cell** is what an algorithm spends on the
//!   content of a bitmap rather than on the bitmap. It compares across
//!   bitmaps holding wildly different amounts.
//! - **Instructions per worked cell** is the same, less the cells that
//!   stand alone. A cell with no neighbour it touches is its own
//!   rectangle in any partition, and both algorithms set those aside
//!   by the same method before they start, so counting them flatters
//!   whichever algorithm is fed the sparsest content rather than
//!   saying anything about either.
//!
//!   There used to be a second metric here. Instruction-optimal bias
//!   multiplied the instructions by what the partition wasted, so that
//!   an algorithm could lose by being slow or by being wasteful and
//!   neither could be read alone. It has nothing left to say: both
//!   algorithms give the minimum on everything measured, so the waste
//!   term is one on both sides and the bias is the instructions again.
//!   It is gone rather than left printing a constant.
//!
//! Counting is callgrind's job, so this drives it. Each shape is run
//! four times -- building the bitmaps, building and reading them,
//! building and partitioning them, and building and solving them
//! exactly -- and the differences are the algorithms alone. Building a
//! sample is not free and has no business in the figure.
//!
//! Run it with no arguments, or with a seed to start from. The same
//! seed gives the same bitmaps, so two runs are comparable down to the
//! instruction; a fresh seed asks whether what the last one showed was
//! about the algorithms or about those bitmaps. It needs `valgrind` on
//! the path.

use bitmatrix::{accurate, samples, RunmaxClipnmerge};

#[path = "common/table.rs"]
mod table;
use table::Table;
use std::process::Command;

/// How many bitmaps a shape is measured over. Enough to average, few
/// enough that both algorithms finish under valgrind: a dense ragged
/// bitmap runs to thousands of rectangles and costs a thousand times
/// what a sparse one does.
const EACH: u64 = 4;

/// One line of a report, header and data alike.
///

/// What a child run is asked to do.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Doing {
    Building,
    /// Setting the lone cells aside, building the runs and finding
    /// the chords: everything both algorithms do before either does
    /// anything of its own. Neither can cost less.
    Reading,
    Partitioning,
    Solving,
}

impl Doing {
    fn word(self) -> &'static str {
        match self {
            Doing::Building => "build",
            Doing::Reading => "read",
            Doing::Partitioning => "partition",
            Doing::Solving => "solve",
        }
    }
}

/// One run. Prints the set cells, the cells standing alone and the
/// rectangles, so the driver has its denominator and its ratio.
fn run(density: f64, cluster: f64, from: u64, doing: Doing) {
    let maps: Vec<_> = samples::grown(from, density, cluster, EACH).collect();
    let cells: u32 = maps.iter().map(|b| b.count_set()).sum();
    // Both algorithms set the cells with no neighbour aside the same
    // way and never partition them, so they are not work either does.
    let lone: u32 = maps.iter().map(|b| b.split_isolated().0.count_set()).sum();
    let areas: usize = match doing {
        Doing::Building => 0,
        Doing::Partitioning => {
            let mut work = RunmaxClipnmerge::new();
            maps.iter().map(|bits| work.partition(bits).len()).sum()
        }
        Doing::Reading => maps.iter().map(|bits| bitmatrix::chords::runs_and_chords(bits)).sum(),
        Doing::Solving => maps.iter().map(|bits| accurate::partition(bits).len()).sum(),
    };
    println!("{cells} {lone} {areas}");
}

/// Runs one pass under callgrind and answers its instructions, set
/// cells, cells standing alone and rectangles.
fn count(density: f64, cluster: f64, from: u64, doing: Doing) -> Option<(u64, u64, u64, u64)> {
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
    let lone = fields.next()?.parse().ok()?;
    let areas = fields.next()?.parse().ok()?;
    Some((took, cells, lone, areas))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    if first.as_deref() == Some("run") {
        let mut number = || args.next().expect("a number").parse::<f64>().expect("a number");
        let (density, cluster, from) = (number(), number(), number() as u64);
        let doing = match args.next().as_deref() {
            Some("read") => Doing::Reading,
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
        let counted =
            [Doing::Building, Doing::Reading, Doing::Partitioning, Doing::Solving]
                .map(|doing| count(shape.density, shape.cluster, from, doing));
        let [
            Some((bare, cells, lone, _)),
            Some((read, ..)),
            Some((mesh, .., areas)),
            Some((solved, .., fewest)),
        ] = counted
        else {
            println!("  {}   (could not run valgrind)", shape.name);
            continue;
        };
        measured.push((
            shape.name,
            cells,
            lone,
            areas,
            fewest,
            mesh - bare,
            solved - bare,
            read - bare,
        ));
    }

    println!(
        "counted under callgrind with the sample build taken out, {EACH} bitmaps a\n\
         shape, seeds from {from}.\n"
    );

    let mut areas_table = Table::new(&[
        "shape",
        "active\ncells",
        "cells\nstanding\nalone",
        "cells the\nalgorithms\nwork on",
        "areas given by\nrunmax-clipnmerge",
        "areas given by\naccurate",
    ]);
    for &(name, cells, lone, areas, fewest, ..) in &measured {
        areas_table.row(&[
            name.to_string(),
            cells.to_string(),
            lone.to_string(),
            (cells - lone).to_string(),
            areas.to_string(),
            fewest.to_string(),
        ]);
    }
    areas_table.print();

    println!(
        "\n  A cell with no neighbour it touches is its own rectangle in any partition,\n  \
         and both algorithms set those aside by the same method before they start, so\n  \
         instructions are priced per cell that is actually partitioned.\n\n  \
         Runs and chords is what both algorithms do before either does anything of\n  \
         its own: set the lone cells aside, build the runs and their transpose, find\n  \
         every chord and settle which to draw. Neither can do less, so the last two\n  \
         columns are what each spends on work of its own.\n"
    );

    let mut table = Table::new(&[
        "shape",
        "cells the\nalgorithms\nwork on",
        "runs and chords\ninstructions\nper worked cell",
        "runmax-clipnmerge\ninstructions\nper worked cell",
        "accurate\ninstructions\nper worked cell",
        "runmax-clipnmerge\nbeyond runs and chords\nper worked cell",
        "accurate\nbeyond runs and chords\nper worked cell",
    ]);
    let per = |count: u64, cells: u64| format!("{:.1}", count as f64 / cells.max(1) as f64);
    let (mut all_worked, mut all_ours, mut all_theirs, mut all_floor) = (0u64, 0u64, 0u64, 0u64);

    for &(name, cells, lone, _, _, ours, theirs, floor) in &measured {
        let worked = cells - lone;
        all_worked += worked;
        all_ours += ours;
        all_theirs += theirs;
        all_floor += floor;
        table.row(&[
            name.to_string(),
            worked.to_string(),
            per(floor, worked),
            per(ours, worked),
            per(theirs, worked),
            per(ours.saturating_sub(floor), worked),
            per(theirs.saturating_sub(floor), worked),
        ]);
    }
    table.rule();
    table.row(&[
        "every shape".to_string(),
        all_worked.to_string(),
        per(all_floor, all_worked),
        per(all_ours, all_worked),
        per(all_theirs, all_worked),
        per(all_ours.saturating_sub(all_floor), all_worked),
        per(all_theirs.saturating_sub(all_floor), all_worked),
    ]);
    table.print();
}
