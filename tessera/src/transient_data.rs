//! `transient_data/`, under the crate's folder and kept out of git: what
//! runs leave behind and the next run reads, in one place, one folder a
//! kind. Nothing here is an input the code needs; a fresh checkout has
//! none of it, and the first run that needs a part of it makes it.
//!
//! | under `transient_data/` | what it holds | written by |
//! |---|---|---|
//! | `seed` | the seed corpus bitmaps grow from, and how many runs have used it | every run that grows a corpus ([`crate::corpus::seed`]) |
//! | `measurements/` | every measuring tool's latest tables, as CSV, with what they were measured on | the diagnostics tools, the searches and the external benchmarks, through [`publish`] |
//! | `worst/` | the worst bitmap each adversarial search has found so far, as PBM | the adversarial searches ([`crate::diagnostics::adversarial::worst`]) |
//! | `renders/` | PNG images of the bitmaps looked at | `diagnostics render` |
//! | `callgrind/` | each instruction count's callgrind output, to see where the instructions go | `diagnostics instruction_count` |
//!
//! Every path is relative to the crate's folder, found from it wherever
//! a run starts from.
//!
//! Function by function: `docs/lab.md`, "`transient_data.rs`".

use crate::corpus::seed::seed_in_use;
use std::path::{Path, PathBuf};
use utilities::table::report::Report;

/// The folder, under the crate's folder.
pub const TRANSIENT_DATA: &str = "transient_data";

/// `relative`, under [`TRANSIENT_DATA`], found from the crate's folder.
fn under(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(TRANSIENT_DATA).join(relative)
}

/// The seed file.
pub fn seed_file() -> PathBuf {
    under("seed")
}

/// Where the measurements are kept.
pub fn measurements() -> PathBuf {
    under("measurements")
}

/// Where the adversarial searches' worst bitmaps are kept.
pub fn worst() -> PathBuf {
    under("worst")
}

/// Where the renders go.
pub fn renders() -> PathBuf {
    under("renders")
}

/// Where the callgrind output goes.
pub fn callgrind() -> PathBuf {
    under("callgrind")
}

/// Notes the seed `report`'s corpus was grown from, if any was, then
/// publishes it in [`measurements`]: printed, and kept in its file.
pub fn publish(mut report: Report) {
    if let Some((seed, fresh)) = seed_in_use() {
        report.note(format!("seed {seed}{}", if fresh { ", fresh for this run" } else { "" }));
    }
    report.publish(&measurements());
}
