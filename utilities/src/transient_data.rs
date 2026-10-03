//! A crate's transient data: `transient_data/`, beside its `Cargo.toml`
//! and kept out of git -- what its runs leave behind, one folder a kind.
//! Nothing there is an input the code needs; a fresh checkout has none of
//! it, and the first run that needs a part makes it.
//!
//! Every crate names its own with [`TransientData::of`] in its
//! `src/transient_data.rs`; its measurements go to `measurements/`, one
//! CSV a tool ([`TransientData::publish`]).

use crate::diagnostics::table::report::Report;
use std::path::{Path, PathBuf};

/// The folder's name, in every crate.
pub const FOLDER: &str = "transient_data";

/// One crate's transient data.
#[derive(Clone, Copy, Debug)]
pub struct TransientData {
    /// The crate's folder: its `CARGO_MANIFEST_DIR`.
    crate_folder: &'static str,
}

impl TransientData {
    /// The transient data of the crate in `crate_folder`:
    /// `env!("CARGO_MANIFEST_DIR")`, from the crate itself.
    pub const fn of(crate_folder: &'static str) -> Self {
        Self { crate_folder }
    }

    /// `relative`, under the crate's `transient_data/`.
    pub fn under(self, relative: &str) -> PathBuf {
        Path::new(self.crate_folder).join(FOLDER).join(relative)
    }

    /// Where the crate's measurements are kept.
    pub fn measurements(self) -> PathBuf {
        self.under("measurements")
    }

    /// Publishes `report` in [`TransientData::measurements`]: printed,
    /// and kept in its file.
    pub fn publish(self, report: Report) {
        report.publish(&self.measurements());
    }
}

/// The utilities' own transient data: what their tests keep.
pub const TRANSIENT_DATA: TransientData = TransientData::of(env!("CARGO_MANIFEST_DIR"));
