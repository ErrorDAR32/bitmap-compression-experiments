//! Where Tessera's measurements are kept -- `docs/measurements/`, under
//! the crate's folder -- and publishing one there, noted with the seed
//! its samples were grown from.

use crate::sample_generators::seed::seed_in_use;
use std::path::{Path, PathBuf};
use utilities::table::report::Report;

/// Where the measurements are kept, under the crate's folder.
pub const MEASUREMENTS: &str = "docs/measurements";

/// [`MEASUREMENTS`], found from the crate's folder, wherever a run
/// starts from.
pub fn folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(MEASUREMENTS)
}

/// Notes the seed `report`'s samples were grown from, if any were, then
/// publishes it in [`folder`]: printed, and kept in its file.
pub fn publish(mut report: Report) {
    if let Some((seed, fresh)) = seed_in_use() {
        report.note(format!("seed {seed}{}", if fresh { ", fresh for this run" } else { "" }));
    }
    report.publish(&folder());
}
