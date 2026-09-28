//! Diagnostics: data gathered from gct's steps and its output, one kind
//! of data to a file. They only gather: nothing here judges a result or
//! prints one. The tests (`tests/`) judge what they gather, and the
//! diagnostics tool (`src/bin/gct_diagnostics/`) prints it.
//!
//! | file | what it gathers |
//! |---|---|
//! | `examination.rs` | one bitmap encoded and decoded: how each cell is said, what is placed where, the complex tiler's bit count and the bits written, the tree read back, the cells decoded |
//! | `measured.rs` | bits, cells set and encode time over many bitmaps |
//! | `tree_stats.rs` | what a tree holds: complex tiles by nesting, masking nodes, complex tiles' bodies |
//! | `census.rs` | a tree's nodes, by kind and level |
//! | `above_complex_tiles.rs` | the divides above the top tiles -- complex tiles, copies, 2x2s -- and what placing those costs |
//! | `bitmaps.rs` | the bitmaps a diagnostic looks at by name: adversarial records, saved bitmaps, one named by the caller |
//! | `png.rs` | a bitmap as a PNG image |

pub mod above_complex_tiles;
pub mod bitmaps;
pub mod census;
pub mod examination;
pub mod measured;
pub mod png;
pub mod tree_stats;

/// The raw cells: what a bitmap costs written out, one bit a cell.
pub const RAW_CELLS: usize = crate::WIDTH * crate::HEIGHT;
