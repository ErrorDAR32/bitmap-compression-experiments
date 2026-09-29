//! The diagnostics tool: prints what `tilesim::diagnostics` gathers from
//! gct, one tool a file, named by the first argument. A tool that
//! measures also keeps its tables, and what they were measured on, in
//! `docs/measurements/<tool>.csv`, replacing the last run's -- the latest
//! numbers are always there, and nowhere copied by hand.
//!
//! | tool | prints |
//! |---|---|
//! | `measurement` | bits a bitmap from every sample generator, one table a generator and one row a parameter set, then what the trees hold, family by family |
//! | `census` | what gct's tree is made of, node kind by level, for each bitmap looked at |
//! | `per_shape` | gct's bits on every shape, plan and line set on its own |
//! | `noise` | gct's bits on noise at several densities, against the raw cells |
//! | `copy_offsets` | a search for better copy offsets, near and far, on the fast sample, the best then set against the current ones on the timed sample |
//! | `sparse` | the tree against the whole bitmap's cell list on sparse bitmaps, density by density, scattered and clustered, beside the least scattered cells can take |
//! | `timing` | wall-clock time to encode and decode, averaged over a large sample, a family at a time |
//! | `instruction_count` | encodes and decodes a fixed sample, for callgrind to count; nothing kept |
//! | `render` | PNG images of the bitmaps looked at, in `target/gct_diagnostics/`; nothing kept |
//! | `show` | the kept measurements, every one or the one named next, read back without measuring |
//!
//! The bitmaps looked at are the adversarial records and saved bitmaps
//! (`external_benchmarks/adversarial/`), plus any PBM image named in `GCT_DIAGNOSE`.
//! Every tool stops if gct loses a cell.
//!
//! ```text
//! cargo run --release --bin gct_diagnostics -- <tool>
//! cargo run --release --bin gct_diagnostics -- show measurement
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

/// A tool that measures: it fills a report, which is printed and kept.
type Measuring = fn(&mut Report);

/// The tools that measure, by name.
const MEASURING: [(&str, Measuring); 7] = [
    ("measurement", measurement::run),
    ("census", census::run),
    ("per_shape", per_shape::run),
    ("noise", noise::run),
    ("copy_offsets", copy_offsets::run),
    ("sparse", sparse::run),
    ("timing", timing::run),
];

/// The tools that keep nothing, by name.
const OTHERS: [(&str, fn()); 3] = [("render", render::run), ("show", show::run), ("instruction_count", instruction_count::run)];

/// The exit code for a tool not named, or named wrongly.
const USAGE_EXIT_CODE: i32 = 2;

/// Runs the tool named by the first argument, or says which there are.
fn main() {
    let asked = std::env::args().nth(1).unwrap_or_default();
    if let Some((name, run)) = MEASURING.iter().find(|(name, _)| *name == asked) {
        let mut report = Report::new(name, &format!("cargo run --release --bin gct_diagnostics -- {name}"));
        run(&mut report);
        report.publish();
    } else if let Some((_, run)) = OTHERS.iter().find(|(name, _)| *name == asked) {
        run();
    } else {
        let names: Vec<&str> = MEASURING.iter().map(|(name, _)| *name).chain(OTHERS.iter().map(|(name, _)| *name)).collect();
        eprintln!("usage: gct_diagnostics <tool>, one of: {}", names.join(", "));
        std::process::exit(USAGE_EXIT_CODE);
    }
}
