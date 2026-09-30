//! The world's data, held in memory in the grains it is read, written
//! and generated in. Nothing here touches the disk yet.
//!
//! | file | what is in it |
//! |---|---|
//! | `coordinates` | where things are: a cell in the world, a superchunk in the world, a chunk in its superchunk, a cell in its chunk, and the conversions between them |
//! | `height_map` | a chunk's heights, one a cell |
//! | `disk_chunk` | a disk chunk: its height map, and its layers, one bitmap a type |
//! | `disk_superchunk` | a disk superchunk: 16x16 disk chunks, the grain of disk access and terrain generation |

mod coordinates;
mod disk_chunk;
mod disk_superchunk;
mod height_map;

pub use coordinates::{
    CellAddress, CellPlace, ChunkPlace, SuperChunkPosition, WorldCell, CHUNKS_IN_SUPERCHUNK, CHUNK_SIDE, SUPERCHUNK_SIDE,
    SUPERCHUNK_SIDE_CELLS,
};
pub use disk_chunk::{DiskChunk, LayerType};
pub use disk_superchunk::DiskSuperChunk;
pub use height_map::{Height, HeightMap};
