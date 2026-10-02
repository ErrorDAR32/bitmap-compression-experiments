//! TileSim: a 2D procedural simulation game, still to come. The world
//! is cut into 256x256 chunks (`chunk_storage/`), each held as layers of
//! bitmaps (`bitmap/`), encoded by Tessera (`tessera/`). What every decision
//! here is weighed against is `docs/design_statements.md`.
//!
//! For now the crate holds its place, so the repository reads as TileSim
//! with its projects beside it.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

/// Nothing yet.
fn main() {}
