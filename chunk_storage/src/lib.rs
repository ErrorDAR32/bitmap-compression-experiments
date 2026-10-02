//! TileSim's chunks as stored (`../docs/tilesim.md`, "Chunk storage"):
//! the cold pool of superchunk images, each one run of words as on disk,
//! and the writeback ring of changed bitmaps that feeds it. Nothing here
//! touches the disk yet. Cells are read and changed in the bitplane
//! manager (`../bitplane_manager`), never here.
//!
//! | file | what is in it |
//! |---|---|
//! | `coordinates` | where things are: a cell in the world, a superchunk in the world, a chunk in its superchunk, a cell in its chunk, the conversions between them, and Morton indices |
//! | `height_map` | a superchunk's heights, one a cell |
//! | `layer_codec` | what a layer is, and the codec that encodes and decodes its bitmap |
//! | `superchunk_image` | a superchunk's words: its chunk table, its height map, its chunks' bitmap tables and bitmaps |
//! | `writeback_ring` | the ring of changed bitmaps, encoded, on their way to the pool |
//! | `chunk_storage` | the pool and the ring together: what the bitplane manager reads from and writes back to |
//! | `mock` | made-up superchunks to try the rest out on: dirt with grass scattered on it |

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

mod chunk_storage;
mod coordinates;
mod height_map;
mod layer_codec;
pub mod mock;
mod superchunk_image;
mod writeback_ring;

pub use chunk_storage::ChunkStorage;
pub use coordinates::{
    CartesianCell, CellAddress, CellIndex, CellPlace, ChunkPlace, ChunkPosition, SuperChunkPosition, CHUNKS_IN_SUPERCHUNK, CHUNK_SIDE, SUPERCHUNK_SIDE,
    SUPERCHUNK_SIDE_CELLS, WORLD_SIDE_SUPERCHUNKS,
};
pub use height_map::{Height, HeightMap, HEIGHT_WORDS};
pub use layer_codec::{LayerCodec, LayerType};
pub use superchunk_image::{InvalidImage, LayerChange, SuperChunkImage};
pub use writeback_ring::{RingEntry, WritebackRing};
