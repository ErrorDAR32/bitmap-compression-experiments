//! `transient_data/`, under the crate's folder and kept out of git: what
//! runs leave behind, in one place, one folder a kind. Nothing here is an
//! input the code needs; a fresh checkout has none of it, and the first
//! run that needs a part of it makes it -- as in Tessera's.
//!
//! | under `transient_data/` | what it holds |
//! |---|---|
//! | `measurements/` | every measurement's latest tables, as CSV, through [`publish`] |
//! | `renders/` | videos of the world ticking (`diagnostics video`) |
//!
//! Every path is relative to the crate's folder, found from it wherever
//! a run starts from.

use std::path::{Path, PathBuf};
use utilities::table::report::Report;

/// The folder, under the crate's folder.
pub const TRANSIENT_DATA: &str = "transient_data";

/// `relative`, under [`TRANSIENT_DATA`], found from the crate's folder.
fn under(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(TRANSIENT_DATA).join(relative)
}

/// Where the measurements are kept.
pub fn measurements() -> PathBuf {
    under("measurements")
}

/// Where the renders go.
pub fn renders() -> PathBuf {
    under("renders")
}

/// Publishes `report` in [`measurements`]: printed, and kept in its file.
pub fn publish(report: Report) {
    report.publish(&measurements());
}
