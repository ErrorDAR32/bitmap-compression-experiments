//! The diagnostics tool: prints what `tilesim::diagnostics` gathers from
//! Tessera, one tool a file, named by the first argument. A tool that
//! measures also keeps its tables, and what they were measured on, in
//! `docs/measurements/<tool>.csv`, replacing the last run's -- the latest
//! numbers are always there, and nowhere copied by hand.
//!
//! Every tool, what it prints, and the argument it takes, are in
//! [`TOOLS`]; run with no tool, or one not there, and they are printed
//! as a table.
//!
//! The bitmaps looked at are the adversarial records and saved bitmaps
//! (`external_benchmarks/adversarial/`), plus any PBM image named in `TESSERA_DIAGNOSE`.
//! Every tool stops if Tessera loses a cell.
//!
//! ```text
//! cargo run --release --bin tessera_diagnostics -- <tool>
//! cargo run --release --bin tessera_diagnostics -- show measurement
//! ```

#![warn(missing_docs, clippy::missing_docs_in_private_items)]

mod census;
mod copy_offsets;
mod instruction_count;
mod measurement;
mod noise;
mod per_shape;
mod render;
mod show;
mod sparse;
mod timing;

use tilesim::table::report::Report;
use tilesim::table::Table;

/// What a tool does when run.
enum Run {
    /// It measures: it fills a report, which is printed and kept.
    Measuring(fn(&mut Report)),
    /// It keeps nothing.
    Other(fn()),
}

/// One tool: its name, what it prints, the argument it takes after its
/// name, if any, and what it does when run.
struct Tool {
    /// What the first argument names it by.
    name: &'static str,
    /// What it prints.
    prints: &'static str,
    /// The argument after its name, and what it is if not given; blank
    /// if it takes none.
    argument: &'static str,
    /// What it does.
    run: Run,
}

/// Every tool.
const TOOLS: [Tool; 11] = [
    Tool {
        name: "measurement",
        prints: "bits a bitmap from every sample generator, a table a generator, a row a parameter set; then what the trees hold",
        argument: "",
        run: Run::Measuring(measurement::run),
    },
    Tool {
        name: "census",
        prints: "what Tessera's tree is made of, node kind by level, for each bitmap looked at",
        argument: "",
        run: Run::Measuring(census::run),
    },
    Tool {
        name: "per_shape",
        prints: "Tessera's bits on every shape, plan and line set on its own",
        argument: "",
        run: Run::Measuring(per_shape::run),
    },
    Tool {
        name: "noise",
        prints: "Tessera's bits on noise at several densities, against the raw cells",
        argument: "",
        run: Run::Measuring(noise::run),
    },
    Tool {
        name: "copy_offsets",
        prints: "a search for better copy offsets on the fast sample, the best set against the current ones on the timed sample",
        argument: "",
        run: Run::Measuring(copy_offsets::run),
    },
    Tool {
        name: "sparse",
        prints: "the tree against the count split on sparse bitmaps, beside the least scattered cells can take",
        argument: "",
        run: Run::Measuring(sparse::run),
    },
    Tool {
        name: "timing",
        prints: "time to encode and decode a large sample, family by family",
        argument: "bitmaps a generator (100)",
        run: Run::Measuring(timing::run),
    },
    Tool {
        name: "instruction_count",
        prints: "instructions to encode and to decode a fixed sample, counted by callgrind (needs valgrind)",
        argument: "",
        run: Run::Measuring(instruction_count::run),
    },
    Tool {
        name: instruction_count::SAMPLE_TOOL,
        prints: "nothing: encodes and decodes that sample alone, uncounted -- what callgrind runs",
        argument: "",
        run: Run::Other(instruction_count::run_sample),
    },
    Tool {
        name: "render",
        prints: "the bitmaps looked at, each written as a PNG image in target/tessera_diagnostics/",
        argument: "",
        run: Run::Other(render::run),
    },
    Tool {
        name: "show",
        prints: "the kept measurements, read back without measuring",
        argument: "a tool's name, for its alone",
        run: Run::Other(show::run),
    },
];

/// The exit code for a tool not named, or named wrongly.
const USAGE_EXIT_CODE: i32 = 2;

/// Runs the tool named by the first argument, or prints every tool.
fn main() {
    let asked = std::env::args().nth(1).unwrap_or_default();
    match TOOLS.iter().find(|tool| tool.name == asked) {
        Some(Tool { name, run: Run::Measuring(run), .. }) => {
            let mut report = Report::new(name, &format!("cargo run --release --bin tessera_diagnostics -- {name}"));
            run(&mut report);
            report.publish();
        }
        Some(Tool { run: Run::Other(run), .. }) => run(),
        None => {
            let kept_in = "kept in\ndocs/measurements/";
            let mut table = Table::new(&["tool", "prints", "argument", kept_in]).left_aligned(&["prints", "argument", kept_in]);
            for tool in &TOOLS {
                let kept = if matches!(tool.run, Run::Measuring(_)) { format!("{}.csv", tool.name) } else { String::new() };
                table.row(&[tool.name, tool.prints, tool.argument, &kept]);
            }
            println!("  cargo run --release --bin tessera_diagnostics -- <tool> [<argument>]");
            table.print();
            std::process::exit(USAGE_EXIT_CODE);
        }
    }
}
