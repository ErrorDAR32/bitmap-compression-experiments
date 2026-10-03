//! Diagnostics: data gathered from Tessera's steps and its output, one kind
//! of data to a file. They only gather: nothing here judges a result or
//! prints one. The tests (`tests/`) judge what they gather, and the
//! diagnostics tool (`src/diagnostics/tool/`) prints it.
//!
//! | file | what it gathers |
//! |---|---|
//! | `examination.rs` | one bitmap encoded and decoded: the bits written, the stream's mode, the first cell decoded wrong |
//! | `measured.rs` | bits, cells set and encode time over many bitmaps |
//! | `tree_stats.rs` | what a tree holds: tiles, complex tiles and their payloads, nodes naming children, cell lists |
//! | `census.rs` | a tree's nodes, by kind and level |
//! | `tool/` | the diagnostics tool itself, a program: one tool a file, printing and keeping what the files here gather |
//! | `adversarial/` | searches for the bitmaps Tessera does worst on, by any score, and the PBM worst bitmaps and saved bitmaps they leave; `main.rs`, the program that runs the search against the raw cells |
//! | `bitmaps.rs` | the bitmaps a diagnostic looks at by name: adversarial worst bitmaps, saved bitmaps, one named by the caller |
//! | `png.rs` | a bitmap as a PNG image |
//!
//! Function by function: `docs/lab.md`, "`diagnostics/`".

pub mod adversarial;
pub mod bitmaps;
pub mod census;
pub mod examination;
pub mod measured;
pub mod png;
pub mod tree_stats;

/// The raw cells: what a bitmap costs written out, one bit a cell.
pub const RAW_CELLS: usize = bitmap::WIDTH * bitmap::HEIGHT;
