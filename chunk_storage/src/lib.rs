//! TileSim's chunks as stored, in the grains they are read, written and
//! generated in (`../docs/tilesim.md`): what loading and saving work on.
//! Nothing here touches the disk yet. Cells are read and changed in the
//! bitplane manager (`../bitplane_manager`), never here.
//!
//! | file | what is in it |
//! |---|---|
//! | `coordinates` | where things are: a cell in the world, a superchunk in the world, a chunk in its superchunk, a cell in its chunk, and the conversions between them |
//! | `height_map` | a chunk's heights, one a cell |
//! | `disk_chunk` | a disk chunk: its height map, and its layers, one a type, encoded; whole layers only, no cells |
//! | `disk_superchunk` | a disk superchunk: 16x16 disk chunks, the grain of disk access and terrain generation |
//! | `encoded_layer` | a layer as a chunk holds it, Tessera-encoded at its exact length, and the codec that encodes and decodes it |

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

mod coordinates;
mod disk_chunk;
mod disk_superchunk;
mod encoded_layer;
mod height_map;

pub use coordinates::{
    CellAddress, CellPlace, ChunkPlace, ChunkPosition, SuperChunkPosition, WorldCell, CHUNKS_IN_SUPERCHUNK, CHUNK_SIDE, SUPERCHUNK_SIDE,
    SUPERCHUNK_SIDE_CELLS,
};
pub use disk_chunk::{DiskChunk, LayerType};
pub use disk_superchunk::DiskSuperChunk;
pub use encoded_layer::{EncodedLayer, LayerCodec};
pub use height_map::{Height, HeightMap};
