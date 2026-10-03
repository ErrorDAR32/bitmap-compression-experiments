//! TileSim: a 2D procedural simulation game. The world is cut into
//! 256x256 chunks (`chunk_storage/`), each held as layers of bitmaps
//! (`bitmap/`), encoded by Tessera (`tessera/`), and made hot to be read
//! and changed in the bitplane manager (`bitplane_manager/`). What every
//! decision here is weighed against is `docs/design_statements.md`.
//!
//! The rules the world runs by, one a module, each a tick of Monte Carlo
//! sampling or entities woken, and the writes and changes it queues:
//!
//! | module | rule |
//! |---|---|
//! | `grass` | grass spreading over dirt |
//! | `sheep` | sheep eating the grass, breeding, walking, starving: the first entity |
//! | `pasture` | grass and sheep, ticked together |
//!
//! Beside them, as in every crate: `diagnostics/`, data gathered from
//! the ticks, printed and kept by the diagnostics tool
//! (`src/bin/diagnostics/`) in `transient_data` (out of git); and the
//! tests, in `tests/`.

//! What TileSim is: `docs/tilesim.md`; function by function:
//! `docs/reference.md`.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

pub mod diagnostics;
pub mod grass;
pub mod pasture;
pub mod sheep;
pub mod transient_data;
