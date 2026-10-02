//! TileSim: a 2D procedural simulation game. The world is cut into
//! 256x256 chunks (`chunk_storage/`), each held as layers of bitmaps
//! (`bitmap/`), encoded by Tessera (`tessera/`), and made hot to be read
//! and changed in the bitplane manager (`bitplane_manager/`). What every
//! decision here is weighed against is `docs/design_statements.md`.
//!
//! The rules the world runs by, one a module, each a tick of Monte Carlo
//! sampling and the writes it queues:
//!
//! | module | rule |
//! |---|---|
//! | `grass` | grass spreading over dirt |

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

pub mod grass;
