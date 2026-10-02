//! A mock world to tick: a square of superchunks from the world's middle,
//! each of dirt with grass scattered on it, every chunk of both hot.

use bitplane_manager::BitmapArena;
use chunk_storage::mock::{grass_on_dirt, DIRT, GRASS};
use chunk_storage::{ChunkPlace, ChunkPosition, ChunkStorage, LayerCodec, SuperChunkPosition, WORLD_SIDE_SUPERCHUNKS};

/// A mock world: its hot bitmaps, its storage, and its superchunks.
pub struct World {
    /// The hot bitmaps.
    pub arena: BitmapArena,
    /// The superchunks as stored.
    pub storage: ChunkStorage,
    /// The superchunks, row by row.
    pub superchunks: Vec<SuperChunkPosition>,
}

impl World {
    /// `count` superchunks in a square, row by row, each with grass drawn
    /// on `grass_cells` cells -- fewer where a cell is drawn twice.
    pub fn grass_on_dirt(count: u32, grass_cells: usize) -> Self {
        let (mut codec, mut arena, mut storage) = (LayerCodec::new(), BitmapArena::new(), ChunkStorage::new(1 << 16));
        let side = (count as f64).sqrt().ceil() as u32;
        let middle = WORLD_SIDE_SUPERCHUNKS / 2;
        let superchunks: Vec<SuperChunkPosition> = (0..count).map(|index| SuperChunkPosition { x: middle + index % side, y: middle + index / side }).collect();
        for (seed, &superchunk) in superchunks.iter().enumerate() {
            storage.insert(superchunk, grass_on_dirt(seed as u64 + 1, grass_cells, &mut codec));
            for place in ChunkPlace::all() {
                arena.make_hot_layers(ChunkPosition::of(superchunk, place), &[DIRT, GRASS], &storage, &mut codec);
            }
        }
        Self { arena, storage, superchunks }
    }

    /// Cells of grass over every superchunk.
    pub fn grass(&self) -> u64 {
        self.superchunks.iter().map(|&superchunk| self.arena.superchunk_count(GRASS, superchunk) as u64).sum()
    }
}
