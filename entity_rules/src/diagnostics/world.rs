//! A mock world to tick: a square of superchunks from the world's middle,
//! each of dirt with grass scattered on it, every chunk of both hot --
//! and, if asked, a flock of sheep on each.

use bitplane_manager::BitmapArena;
use chunk_storage::mock::{grass_on_dirt, DIRT, GRASS};
use chunk_storage::{ChunkStorage, LayerCodec};
use coordinates::{ChunkPlace, ChunkPosition, SuperchunkPosition, WORLD_SIDE_SUPERCHUNKS};
use crate::sheep::{flock, SHEEP};
use simulation::entity_store::Entities;
use utilities::rng::Rng;

/// A mock world: its hot bitmaps, its storage, and its superchunks.
pub struct MockWorld {
    /// The hot bitmaps.
    pub arena: BitmapArena,
    /// The entities, holding the same superchunks as the arena.
    pub entities: Entities,
    /// The superchunks as stored.
    pub storage: ChunkStorage,
    /// The superchunks, row by row.
    pub superchunks: Vec<SuperchunkPosition>,
}

impl MockWorld {
    /// `count` superchunks in a square, row by row, each with grass drawn
    /// on `grass_cells` cells -- fewer where a cell is drawn twice.
    pub fn grass_on_dirt(count: u32, grass_cells: usize) -> Self {
        let (mut codec, mut arena, mut storage) = (LayerCodec::new(), BitmapArena::new(), ChunkStorage::new(1 << 16));
        let side = (count as f64).sqrt().ceil() as u32;
        let middle = WORLD_SIDE_SUPERCHUNKS / 2;
        let superchunks: Vec<SuperchunkPosition> = (0..count).map(|index| SuperchunkPosition { x: middle + index % side, y: middle + index / side }).collect();
        for (seed, &superchunk) in superchunks.iter().enumerate() {
            storage.insert(superchunk, grass_on_dirt(seed as u64 + 1, grass_cells, &mut codec));
            for place in ChunkPlace::all() {
                arena.make_hot_layers(ChunkPosition::of(superchunk, place), &[DIRT, GRASS], &storage, &mut codec);
            }
        }
        let mut entities = Entities::new();
        let superchunk_indices: Vec<u64> = arena.superchunks().iter().map(|superchunk| superchunk.morton_index()).collect();
        entities.align(&superchunk_indices);
        Self { arena, entities, storage, superchunks }
    }

    /// [`MockWorld::grass_on_dirt`], with `sheep` sheep on each superchunk,
    /// on cells drawn at random.
    pub fn with_sheep(count: u32, grass_cells: usize, sheep: usize) -> Self {
        let mut world = Self::grass_on_dirt(count, grass_cells);
        let mut random = Rng::new(0x5EE9);
        for &superchunk in &world.superchunks {
            flock(&mut world.entities, superchunk, sheep, &mut random);
        }
        world.entities.apply();
        world
    }

    /// Sheep over every superchunk.
    pub fn sheep(&self) -> usize {
        self.entities.iter().filter(|entity| entity.header.kind == SHEEP).count()
    }

    /// Cells of grass over every superchunk.
    pub fn grass(&self) -> u64 {
        self.superchunks.iter().map(|&superchunk| self.arena.superchunk_count(GRASS, superchunk) as u64).sum()
    }
}
