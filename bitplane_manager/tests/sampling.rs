//! Monte Carlo sampling: every set cell chosen with the probability
//! asked, only set cells, each once, in Morton order, weighted across
//! chunks by their counts, and only hot bitmaps.
//!
//! `cargo test`

use bitplane_manager::{BitmapArena, BucketKey, Random, Shape, Write, WriteOp};
use chunk_storage::{ChunkPosition, LayerCodec, LayerType, WorldCell};

/// The layer type the tests sample.
const STONE: LayerType = LayerType(4);

/// An arena with `STONE` hot in the chunks at `chunks`, the cells of
/// `shapes` set.
fn arena_with(chunks: &[ChunkPosition], shapes: &[Shape]) -> BitmapArena {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    for &chunk in chunks {
        arena.make_hot(BucketKey { layer_type: STONE, chunk }, None, &mut codec);
    }
    for &shape in shapes {
        arena.queue(Write { layer_type: STONE, op: WriteOp::Set, shape });
    }
    assert_eq!(arena.apply().missed, 0);
    arena
}

/// Every cell sampled, with `probability`.
fn sampled(arena: &BitmapArena, probability: f64, seed: u64) -> Vec<WorldCell> {
    let mut cells = Vec::new();
    let count = arena.sample(STONE, probability, &mut Random::new(seed), |cell| cells.push(cell));
    assert_eq!(count, cells.len());
    cells
}

/// The chunks of the two superchunks the tests use, side by side.
fn two_superchunks() -> Vec<ChunkPosition> {
    (0..4).flat_map(|y| (0..8).map(move |x| ChunkPosition { x: 400 + x, y: 400 + y })).collect()
}

/// At probability 1 every set cell comes once, in Morton order across
/// superchunks; at 0, none.
#[test]
fn certain_sampling_finds_every_set_cell_in_morton_order() {
    let shapes = [
        Shape::Disc { center: WorldCell { x: 102_900, y: 102_600 }, radius: 40 },
        Shape::Rect { corner: WorldCell { x: 103_300, y: 103_000 }, width: 200, height: 3 },
        Shape::Cell(WorldCell { x: 102_400, y: 102_400 }),
    ];
    let arena = arena_with(&two_superchunks(), &shapes);
    let cells = sampled(&arena, 1.0, 1);
    let expected: usize = two_superchunks().iter().map(|&chunk| arena.bucket(BucketKey { layer_type: STONE, chunk }).expect("hot").count() as usize).sum();
    assert_eq!(cells.len(), expected);
    assert!(cells.windows(2).all(|pair| pair[0].morton_index() < pair[1].morton_index()), "in Morton order, each once");
    assert!(cells.iter().all(|&cell| arena.holds(STONE, cell) == Ok(true)), "only set cells");
    assert!(sampled(&arena, 0.0, 1).is_empty());
}

/// Each set cell is chosen with the probability asked: over a million
/// set cells at 1%, the count is within five standard deviations of
/// 10,486; every cell chosen is set, and they come in Morton order.
#[test]
fn each_cell_is_chosen_with_the_probability_asked() {
    let chunks: Vec<ChunkPosition> = (0..4).flat_map(|y| (0..4).map(move |x| ChunkPosition { x: 400 + x, y: 400 + y })).collect();
    let arena = arena_with(&chunks, &[Shape::Rect { corner: WorldCell { x: 102_400, y: 102_400 }, width: 1024, height: 1024 }]);
    for seed in 1..=3 {
        let cells = sampled(&arena, 0.01, seed);
        let (mean, deviation) = (1_048_576.0 * 0.01, (1_048_576.0f64 * 0.01 * 0.99).sqrt());
        assert!((cells.len() as f64 - mean).abs() < 5.0 * deviation, "seed {seed}: {} cells", cells.len());
        assert!(cells.windows(2).all(|pair| pair[0].morton_index() < pair[1].morton_index()));
    }
}

/// Samples fall on chunks in proportion to their set cells: a chunk
/// full and one a sixteenth full get them sixteen to one.
#[test]
fn chunks_are_weighted_by_their_counts() {
    let (full, sparse) = (ChunkPosition { x: 400, y: 400 }, ChunkPosition { x: 401, y: 400 });
    let shapes = [
        Shape::Rect { corner: WorldCell { x: 102_400, y: 102_400 }, width: 256, height: 256 },
        Shape::Rect { corner: WorldCell { x: 102_656, y: 102_400 }, width: 64, height: 64 },
    ];
    let arena = arena_with(&[full, sparse], &shapes);
    let cells = sampled(&arena, 0.05, 7);
    let in_sparse = cells.iter().filter(|cell| cell.chunk_and_cell().0 == sparse).count() as f64;
    let ratio = (cells.len() as f64 - in_sparse) / in_sparse;
    assert!((14.0..18.5).contains(&ratio), "{ratio:.2} to one");
}

/// Only hot bitmaps are sampled: an evicted chunk's cells are not.
#[test]
fn only_hot_bitmaps_are_sampled() {
    let (kept, evicted) = (ChunkPosition { x: 400, y: 400 }, ChunkPosition { x: 401, y: 400 });
    let mut arena = arena_with(&[kept, evicted], &[Shape::Rect { corner: WorldCell { x: 102_400, y: 102_400 }, width: 512, height: 4 }]);
    assert_eq!(sampled(&arena, 1.0, 1).len(), 2048);
    let mut storage = chunk_storage::ChunkStorage::new(1 << 12);
    arena.write_back(evicted.superchunk_and_place().0, &mut storage, &mut LayerCodec::new());
    assert!(arena.evict(BucketKey { layer_type: STONE, chunk: evicted }));
    assert!(sampled(&arena, 1.0, 1).iter().all(|cell| cell.chunk_and_cell().0 == kept));
    assert_eq!(sampled(&arena, 1.0, 1).len(), 1024);
}
