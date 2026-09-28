//! The diagnostics tool: prints what `bitmap::diagnostics` gathers from
//! gct, one tool a file, named by the first argument:
//!
//! | tool | prints |
//! |---|---|
//! | `measurement` | bits a bitmap from every sample generator, one table a generator and one row a parameter set, then what the trees hold, family by family |
//! | `census` | what gct's tree is made of, node kind by level, for each bitmap looked at |
//! | `above` | the divides above the top tiles, family by family, against listing those tiles in Morton order |
//! | `per_shape` | gct's bits on every shape, plan and line set on its own |
//! | `noise` | gct's bits on noise at several densities, against the raw cells |
//! | `render` | PNG images of the bitmaps looked at, in `target/gct_diagnostics/` |
//!
//! The bitmaps looked at are the adversarial records and saved bitmaps
//! (`testing/adversarial/`), plus any PBM image named in `GCT_DIAGNOSE`.
//! Every tool stops if gct loses a cell.
//!
//! ```text
//! cargo run --release --bin gct_diagnostics -- <tool>
//! ```

#![warn(missing_docs, clippy::missing_docs_in_private_items)]

mod above;
mod census;
mod measurement;
mod noise;
mod per_shape;
mod render;

/// Every tool, by name.
const TOOLS: [(&str, fn()); 6] = [
    ("measurement", measurement::run),
    ("census", census::run),
    ("above", above::run),
    ("per_shape", per_shape::run),
    ("noise", noise::run),
    ("render", render::run),
];

/// The exit code for a tool not named, or named wrongly.
const USAGE_EXIT_CODE: i32 = 2;

/// Runs the tool named by the first argument, or says which there are.
fn main() {
    let asked = std::env::args().nth(1).unwrap_or_default();
    match TOOLS.iter().find(|(name, _)| *name == asked) {
        Some((_, run)) => run(),
        None => {
            let names: Vec<&str> = TOOLS.iter().map(|(name, _)| *name).collect();
            eprintln!("usage: gct_diagnostics <tool>, one of: {}", names.join(", "));
            std::process::exit(USAGE_EXIT_CODE);
        }
    }
}
