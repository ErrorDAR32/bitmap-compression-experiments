//! TileSim's chunks as stored (`../docs/tilesim.md`, "Chunk storage"):
//! the cold pool of superchunk images, each one run of words as on disk,
//! and the writeback ring of changed bitmaps that feeds it. Nothing here
//! touches the disk yet. Cells are read and changed in the bitplane
//! manager (`../bitplane_manager`), never here.
//!
//! | file | what is in it |
//! |---|---|
//! | `height_map` | a superchunk's heights, one a cell |
//! | `layer_codec` | what a layer is, and the codec that encodes and decodes its bitmap |
//! | `superchunk_image` | a superchunk's words: its chunk table, its height map, its chunks' bitmap tables and bitmaps |
//! | `writeback_ring` | the ring of changed bitmaps, encoded, on their way to the cold pool |
//! | `chunk_storage` | the cold pool and the ring together: what the bitplane manager reads from and writes back to |
//! | `mock` | made-up superchunks to try the rest out on: dirt with grass scattered on it |
//! | `diagnostics/` | data gathered from storage, judged by the tests and printed by tools |
//! | `transient_data` | where runs leave what they make, out of git |

//! The design: `docs/chunk_storage.md`; function by function:
//! `docs/reference.md`.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

mod chunk_storage;
pub mod diagnostics;
pub mod disk;
mod height_map;
mod layer_codec;
pub mod mock;
mod superchunk_image;
pub mod transient_data;
mod writeback_ring;

pub use chunk_storage::ChunkStorage;
pub use height_map::{Height, HeightMap, HEIGHT_WORDS};
pub use layer_codec::{LayerCodec, LayerType};
pub use superchunk_image::{InvalidImage, LayerChange, SuperchunkImage};
pub use writeback_ring::{RingEntry, WritebackRing};
