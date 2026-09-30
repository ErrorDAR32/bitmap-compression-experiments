//! TileSim: a 2D procedural simulation game (`docs/tilesim.md`). Every
//! decision here is weighed against `docs/design_statements.md`.
//!
//! | module | what it is |
//! |---|---|
//! | [`world`] | the world's data in memory: disk chunks, disk superchunks, and the coordinates between them |

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

pub mod world;
