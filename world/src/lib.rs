//! TileSim's world as a whole: made from a seed ([`generate`]), ticked
//! -- its cells' rules (`mc_rules/`) and its entities (`entity_rules/`)
//! together ([`tick`]) -- saved ([`save`]) and loaded ([`load`]) as
//! it was, to the cell and the random number. Where a save's files are
//! and what they hold: `chunk_storage::disk`. The design:
//! `docs/world.md`; function by function: `docs/reference.md`.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

pub mod diagnostics;
mod tick;
pub mod transient_data;

pub use tick::{tick, TickCounts};

use bitplane_manager::BitmapArena;
use chunk_storage::disk::{self, DiskError, WorldInfo};
use chunk_storage::{ChunkStorage, HeightMap, LayerChange, LayerCodec, LayerType, SuperchunkImage};
use terrain::{Terrain, WALLS};
use chunk_storage::mock::{grass_on_dirt, DIRT, GRASS};
use coordinates::{ChunkPlace, ChunkPosition, SuperchunkPosition, WORLD_SIDE_SUPERCHUNKS};
use entity_rules::sheep::flock;
use simulation::entity_store::{saved, Entities};
use simulation::Simulation;
use std::path::Path;
use utilities::rng::Rng;

/// Cells of grass drawn on a superchunk generated: a third of it, less those drawn twice.
const GRASS_CELLS: usize = 400_000;
/// Sheep on a superchunk generated.
const FLOCK: usize = 4_000;

/// What a save wrote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Saved {
    /// Superchunks written.
    pub superchunks: usize,
    /// Entities written.
    pub entities: usize,
    /// Bytes written, every file.
    pub bytes: u64,
}

/// A world read back.
pub struct World {
    /// What the world is, and the tick it is at.
    pub info: WorldInfo,
    /// Its hot bitmaps: every layer type of every superchunk.
    pub arena: BitmapArena,
    /// Its superchunks as stored.
    pub storage: ChunkStorage,
    /// Its entities, at its tick.
    pub entities: Entities,
    /// Its simulation, each superchunk's random numbers taken up.
    pub simulation: Simulation,
}

/// A world made from `seed`, `superchunks` of them in a square from the
/// world's middle: its terrain -- heights, and the walls they make --
/// and on it, for now, pasture: dirt, a third of it grass, and a flock
/// on each. Every superchunk's from the seed and where it is.
pub fn generate(seed: u64, superchunks: u32) -> World {
    generate_with(seed, superchunks, GRASS_CELLS, FLOCK)
}

/// [`generate`], with `grass_cells` cells of grass drawn and `flock`
/// sheep on each superchunk.
pub fn generate_with(seed: u64, superchunks: u32, grass_cells: usize, flock_size: usize) -> World {
    let (mut codec, mut arena, mut storage) = (LayerCodec::new(), BitmapArena::new(), ChunkStorage::new(1 << 16));
    let side = (superchunks as f64).sqrt().ceil() as u32;
    let middle = WORLD_SIDE_SUPERCHUNKS / 2;
    let layers: Vec<LayerType> = [DIRT, GRASS].into_iter().chain(WALLS.map(|(layer_type, _)| layer_type)).collect();
    let made: Vec<SuperchunkPosition> = (0..superchunks).map(|index| SuperchunkPosition { x: middle + index % side, y: middle + index / side }).collect();
    for &superchunk in &made {
        let own = Rng::for_stream(seed, superchunk.morton_index()).draw();
        let terrain = Terrain::generate(seed, superchunk);
        // Each way's walls, a layer a chunk that has any.
        let mut walls: Vec<(usize, LayerType, Vec<u64>)> = Vec::new();
        for (way, &(layer_type, _)) in WALLS.iter().enumerate() {
            for (chunk, cells) in terrain.walls[way].iter().enumerate().filter(|(_, cells)| cells.iter().any(|&word| word != 0)) {
                walls.push((chunk, layer_type, codec.encode(cells).to_vec()));
            }
        }
        let changes: Vec<LayerChange> = walls.iter().map(|(chunk, layer_type, words)| LayerChange { chunk: *chunk, layer_type: *layer_type, encoded: words }).collect();
        storage.insert(superchunk, grass_on_dirt(own, grass_cells, &mut codec).with_heights(&terrain.heights).rewritten(&changes));
        for place in ChunkPlace::all() {
            arena.make_hot_layers(ChunkPosition::of(superchunk, place), &layers, &storage, &mut codec);
        }
    }
    let mut entities = Entities::new();
    let superchunk_indices: Vec<u64> = arena.superchunks().iter().map(|superchunk| superchunk.morton_index()).collect();
    entities.align(&superchunk_indices);
    for &superchunk in &made {
        let mut own = Rng::for_stream(!seed, superchunk.morton_index());
        flock(&mut entities, superchunk, flock_size, &mut own);
    }
    entities.apply();
    let info = WorldInfo { name: String::new(), seed, tick: 0, layers };
    World { info, arena, storage, entities, simulation: Simulation::for_superchunks(made.len()) }
}

/// Saves the world in `folder`, made if not there: `name` and `seed`
/// what it is, the tick `entities`' -- between two ticks. Every dirty
/// bitmap is written back and the ring flushed first, so the cold
/// pool's images are the world's cells.
pub fn save(folder: &Path, name: &str, seed: u64, arena: &mut BitmapArena, storage: &mut ChunkStorage, entities: &Entities, simulation: &Simulation) -> Result<Saved, DiskError> {
    let mut codec = LayerCodec::new();
    let hot: Vec<SuperchunkPosition> = arena.superchunks().iter().map(|superchunk| superchunk.position()).collect();
    for &superchunk in &hot {
        arena.write_back(superchunk, storage, &mut codec);
    }
    let mut flushed = Vec::new();
    storage.flush_all(&mut flushed);
    arena.flushed(&flushed);
    for &superchunk in &hot {
        // One with no cell ever set has no image yet: saved all the same.
        if storage.image(superchunk).is_none() {
            storage.insert(superchunk, SuperchunkImage::new(&HeightMap::default()));
        }
    }
    let mut layers: Vec<LayerType> = arena.keys().map(|key| key.layer_type).collect();
    layers.sort_unstable();
    layers.dedup();

    let mut saved = Saved::default();
    let random: Vec<(u64, u64)> = simulation.random_states().collect();
    for superchunk in storage.superchunks() {
        let superchunk_index = superchunk.morton_index();
        saved.bytes += disk::write_image(folder, superchunk, storage.image(superchunk).expect("a superchunk of the cold pool"))?;
        let state = random.binary_search_by_key(&superchunk_index, |state| state.0).ok().map(|at| random[at].1);
        let (words, count) = saved::encode_state(state, entities.superchunk(superchunk_index));
        saved.bytes += disk::write_state(folder, superchunk, &words)?;
        saved.superchunks += 1;
        saved.entities += count;
    }
    // The world's file last: a save cut short leaves the one before it.
    saved.bytes += disk::write_world(folder, &WorldInfo { name: name.to_string(), seed, tick: entities.now(), layers })?;
    Ok(saved)
}

/// Loads the world saved in `folder`: every superchunk read into the
/// cold pool, every layer type of it made hot, its entities put back at
/// its tick, its random streams taken up -- as it was when saved, to the
/// cell and the random number.
pub fn load(folder: &Path) -> Result<World, DiskError> {
    let info = disk::read_world(folder)?;
    let superchunks = disk::saved_superchunks(folder)?;
    let (mut codec, mut arena, mut storage) = (LayerCodec::new(), BitmapArena::new(), ChunkStorage::new(1 << 16));
    for &superchunk in &superchunks {
        storage.insert(superchunk, disk::read_image(folder, superchunk)?);
        for place in ChunkPlace::all() {
            arena.make_hot_layers(ChunkPosition::of(superchunk, place), &info.layers, &storage, &mut codec);
        }
    }
    let mut entities = Entities::at_tick(info.tick);
    let superchunk_indices: Vec<u64> = arena.superchunks().iter().map(|superchunk| superchunk.morton_index()).collect();
    entities.align(&superchunk_indices);
    let (mut random, mut crossings, mut expected) = (Vec::new(), Vec::new(), 0);
    for &superchunk in &superchunks {
        let (words, path) = disk::read_state(folder, superchunk)?;
        let state = saved::decode_state(&words, info.tick, &mut entities, &mut crossings).map_err(|what| DiskError::Invalid(path, what.to_string()))?;
        random.extend(state.random.map(|state| (superchunk.morton_index(), state)));
        expected += state.entities;
    }
    let invalid = |what: String| DiskError::Invalid(folder.to_path_buf(), what);
    let applied = entities.apply();
    if applied.puts != expected || applied.lost + applied.refused != 0 {
        return Err(invalid(format!("{} of {expected} entities put back, {} on no superchunk saved, {} on a cell taken", applied.puts, applied.lost, applied.refused)));
    }
    if let Some(crossing) = crossings.into_iter().find(|&crossing| !entities.restore_crossing(crossing)) {
        return Err(invalid(format!("{crossing:?} on no superchunk saved")));
    }
    let mut simulation = Simulation::for_superchunks(superchunks.len());
    simulation.restore_random(&random);
    Ok(World { info, arena, storage, entities, simulation })
}
