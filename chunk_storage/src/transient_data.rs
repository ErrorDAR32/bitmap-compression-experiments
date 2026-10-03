//! `transient_data/`, under the crate's folder and kept out of git: what
//! its runs leave behind ([`utilities::transient_data`]).
//!
//! | under `transient_data/` | what it holds |
//! |---|---|
//! | `measurements/` | every measurement's latest tables, as CSV |

use utilities::transient_data::TransientData;

/// The crate's transient data.
pub const TRANSIENT_DATA: TransientData = TransientData::of(env!("CARGO_MANIFEST_DIR"));
