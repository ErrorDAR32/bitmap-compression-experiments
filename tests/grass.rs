//! Grass over dirt: it spreads onto dirt at its chance, decays at its
//! chance times its share of grass neighbours, and every cell stays dirt
//! or grass.
//!
//! `cargo test`

use bitplane_manager::{BitmapArena, Shape, Write, WriteOp};
use chunk_storage::mock::{grass_on_dirt, DIRT, GRASS};
use chunk_storage::{ChunkStorage, LayerCodec};
use coordinates::{CartesianCell, ChunkPlace, ChunkPosition, SuperChunkPosition, SUPERCHUNK_SIDE_CELLS};
use simulation::entities::Entities;
use simulation::Simulation;
use tilesim::grass::{tick, DECAY_CHANCE, SPREAD_CHANCE};

/// The superchunk the tests run on.
const SUPERCHUNK: SuperChunkPosition = SuperChunkPosition { x: 3, y: 3 };

/// Cells in a superchunk.
const CELLS: u32 = 1 << 20;

/// An arena with the mock superchunk hot, `grass_cells` cells of grass
/// scattered on its dirt.
fn mock(grass_cells: usize) -> BitmapArena {
    let (mut codec, mut arena, mut storage) = (LayerCodec::new(), BitmapArena::new(), ChunkStorage::new(1 << 12));
    storage.insert(SUPERCHUNK, grass_on_dirt(5, grass_cells, &mut codec));
    for place in ChunkPlace::all() {
        arena.make_hot_layers(ChunkPosition::of(SUPERCHUNK, place), &[DIRT, GRASS], &storage, &mut codec);
    }
    arena
}

/// Turns the cells of `writes`' shapes to grass.
fn plant(arena: &mut BitmapArena, writes: impl Iterator<Item = (CartesianCell, Shape)>) {
    for (at, shape) in writes {
        arena.queue(GRASS, Write { at: at.into(), op: WriteOp::Set, shape });
        arena.queue(DIRT, Write { at: at.into(), op: WriteOp::Unset, shape });
    }
    assert_eq!(arena.apply().missed, 0);
}

/// The superchunk's first cell, at its top left.
fn origin() -> CartesianCell {
    CartesianCell { x: SUPERCHUNK.x * SUPERCHUNK_SIDE_CELLS, y: SUPERCHUNK.y * SUPERCHUNK_SIDE_CELLS }
}

/// Grass alone, with no grass around, never decays: a lattice of grass
/// a cell every 16 each way, for a tick.
#[test]
fn lone_grass_never_decays() {
    let mut arena = mock(0);
    let cells = (0..64).flat_map(|y| (0..64).map(move |x| CartesianCell { x: origin().x + 16 * x, y: origin().y + 16 * y }));
    plant(&mut arena, cells.map(|cell| (cell, Shape::Cell)));
    assert_eq!(arena.superchunk_count(GRASS, SUPERCHUNK), 4096);
    let done = tick(&mut Simulation::new(1), &mut arena, &mut Entities::new(), 3).rules;
    assert!(done.sampled > 0);
    assert_eq!(done.decays, 0);
}

/// Grass with grass all round decays at the whole chance, and has no
/// dirt to spread onto: a superchunk all grass loses about 0.2% of it in
/// a tick -- a little less, as cells on its edge see neighbours past it
/// that are not hot.
#[test]
fn surrounded_grass_decays_at_its_chance() {
    let mut arena = mock(0);
    let pieces = (0..8).flat_map(|y| (0..8).map(move |x| CartesianCell { x: origin().x + 128 * x, y: origin().y + 128 * y }));
    plant(&mut arena, pieces.map(|at| (at, Shape::Rect { width: 128, height: 128 })));
    assert_eq!(arena.superchunk_count(GRASS, SUPERCHUNK), CELLS);
    let done = tick(&mut Simulation::new(1), &mut arena, &mut Entities::new(), 4).rules;
    let expected = CELLS as f64 * DECAY_CHANCE;
    assert_eq!(done.spreads, 0);
    assert!((done.decays as f64 / expected - 1.0).abs() < 0.15, "{} decays, about {expected:.0} expected", done.decays);
    assert_eq!(arena.superchunk_count(GRASS, SUPERCHUNK), CELLS - done.decays as u32);
}

/// Over 1,000 ticks every cell stays dirt or grass, the grass changes by
/// no more than what spread and decayed, and scattered grass grows --
/// at most by e, what spreading alone would make of it.
#[test]
fn every_cell_stays_dirt_or_grass() {
    let mut arena = mock(400);
    let start = arena.superchunk_count(GRASS, SUPERCHUNK);
    let (mut grass, mut simulation) = (start, Simulation::new(1));
    for seed in 0..1000 {
        let done = tick(&mut simulation, &mut arena, &mut Entities::new(), seed).rules;
        let now = arena.superchunk_count(GRASS, SUPERCHUNK);
        assert!(now + done.decays as u32 >= grass && now + done.decays as u32 <= grass + done.spreads as u32, "grown by what spread, less what decayed");
        assert_eq!(now + arena.superchunk_count(DIRT, SUPERCHUNK), CELLS, "dirt or grass");
        grass = now;
    }
    let growth = grass as f64 / start as f64;
    assert!(growth > 1.0 && growth < (1000.0 * SPREAD_CHANCE).exp() * 1.1, "grew {growth:.2} times");
}
