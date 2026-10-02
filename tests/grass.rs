//! Grass spreading over the mock superchunk: it only grows, only onto
//! dirt, at about the chance a tick asked.
//!
//! `cargo test`

use bitplane_manager::{BitmapArena, Random};
use chunk_storage::mock::{grass_on_dirt, DIRT, GRASS};
use chunk_storage::{ChunkPlace, ChunkPosition, ChunkStorage, LayerCodec, SuperChunkPosition};
use tilesim::grass::{tick, SPREAD_CHANCE};

/// Over 1,000 ticks grass never shrinks, every cell stays dirt or
/// grass, and the grass grows by about e: each cell spreads at the
/// chance a tick, nearly always onto dirt while the grass is sparse.
#[test]
fn grass_spreads_at_its_chance() {
    let superchunk = SuperChunkPosition { x: 3, y: 3 };
    let (mut codec, mut arena, mut storage) = (LayerCodec::new(), BitmapArena::new(), ChunkStorage::new(1 << 12));
    storage.insert(superchunk, grass_on_dirt(5, 400, &mut codec));
    for place in ChunkPlace::all() {
        arena.make_hot_layers(ChunkPosition::of(superchunk, place), &[DIRT, GRASS], &storage, &mut codec);
    }
    let start = arena.superchunk_count(GRASS, superchunk);
    let (mut random, mut samples, mut grass) = (Random::new(2), Vec::new(), start);
    for _ in 0..1000 {
        let done = tick(&mut arena, &mut random, &mut samples);
        let now = arena.superchunk_count(GRASS, superchunk);
        assert_eq!(now, grass + done.spread as u32, "grown by what spread");
        assert_eq!(now + arena.superchunk_count(DIRT, superchunk), 1 << 20, "dirt or grass");
        grass = now;
    }
    let growth = grass as f64 / start as f64;
    let expected = (1000.0 * SPREAD_CHANCE).exp();
    assert!((growth / expected - 1.0).abs() < 0.15, "grew {growth:.2} times, about {expected:.2} expected");
}
