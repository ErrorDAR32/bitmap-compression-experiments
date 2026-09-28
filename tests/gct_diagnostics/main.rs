//! Diagnostics: tools for looking inside gct's results, not tests of
//! it. Each is an ignored test in a file of its own, printing a table
//! or writing images:
//!
//! | file | tool |
//! |---|---|
//! | `census.rs` | what gct's tree is made of, node kind by level |
//! | `per_shape.rs` | gct's bits on every shape, plan and line set on its own |
//! | `noise.rs` | gct's bits on noise at several densities, against the raw cells |
//! | `render.rs` | PNG images of the bitmaps looked at, in `target/gct_diagnostics/` |
//!
//! They look at the adversarial records and saved patterns (`testing/adversarial/`), plus
//! any PBM image named in `GCT_DIAGNOSE` (`bitmaps.rs`).
//!
//! `cargo test --release --test gct_diagnostics -- --ignored --nocapture <tool>`

mod bitmaps;
mod census;
mod noise;
mod per_shape;
mod render;
