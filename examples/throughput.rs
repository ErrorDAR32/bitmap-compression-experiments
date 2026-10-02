//! How many writes a second one core handles, ticking grass -- spreading
//! and decay -- as fast as it goes: each step of the tick timed apart -- sampling, computing
//! the writes, applying them.
//!
//! `cargo run --release --example throughput -- [ticks] [grass, in thousandths of the cells] [superchunks]`
//!
//! The grass starts scattered at the share asked, so ticks have samples
//! and writes from the first; it grows as it runs.

use bitplane_manager::{BitmapArena, Random};
use chunk_storage::mock::{grass_on_dirt, DIRT, GRASS};
use chunk_storage::{ChunkPlace, ChunkPosition, ChunkStorage, LayerCodec, SuperChunkPosition, WORLD_SIDE_SUPERCHUNKS};
use std::time::{Duration, Instant};
use tilesim::grass;

/// Runs the ticks asked for, flat out, and prints what each step took.
fn main() {
    let arguments: Vec<usize> = std::env::args().skip(1).map(|argument| argument.parse().expect("a number")).collect();
    let ticks = arguments.first().copied().unwrap_or(10_000);
    let thousandths = arguments.get(1).copied().unwrap_or(250);
    let superchunk_count = arguments.get(2).copied().unwrap_or(1) as u32;

    let (mut codec, mut arena, mut storage) = (LayerCodec::new(), BitmapArena::new(), ChunkStorage::new(1 << 16));
    let side = (superchunk_count as f64).sqrt().ceil() as u32;
    let superchunks: Vec<SuperChunkPosition> = (0..superchunk_count)
        .map(|index| SuperChunkPosition { x: WORLD_SIDE_SUPERCHUNKS / 2 + index % side, y: WORLD_SIDE_SUPERCHUNKS / 2 + index / side })
        .collect();
    for (seed, &superchunk) in superchunks.iter().enumerate() {
        // Drawn cells may repeat, so a little under the share asked.
        storage.insert(superchunk, grass_on_dirt(seed as u64 + 1, (1 << 20) * thousandths / 1000, &mut codec));
        for place in ChunkPlace::all() {
            arena.make_hot_layers(ChunkPosition::of(superchunk, place), &[DIRT, GRASS], &storage, &mut codec);
        }
    }
    let grass_now = |arena: &BitmapArena| superchunks.iter().map(|&superchunk| arena.superchunk_count(GRASS, superchunk) as u64).sum::<u64>();
    let grass_at_start = grass_now(&arena);

    let (mut random, mut samples) = (Random::new(1), Vec::new());
    let (mut sampling, mut computing, mut applying) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let (mut sampled, mut writes, mut changed) = (0, 0, 0);
    for _ in 0..ticks {
        let start = Instant::now();
        sampled += grass::sample(&arena, &mut random, &mut samples);
        let sampled_at = Instant::now();
        let (spreads, decays) = grass::compute(&mut arena, &mut random, &samples);
        writes += 2 * (spreads + decays);
        let computed_at = Instant::now();
        changed += arena.apply().changed;
        let applied_at = Instant::now();
        sampling += sampled_at - start;
        computing += computed_at - sampled_at;
        applying += applied_at - computed_at;
    }
    let total = sampling + computing + applying;

    println!("{ticks} ticks over {superchunk_count} superchunk(s); grass {grass_at_start} -> {}", grass_now(&arena));
    println!("samples {sampled}, writes {writes}, cells changed {changed}: {:.0} writes a tick", writes as f64 / ticks as f64);
    println!("{:<20} {:>12} {:>10} {:>14}", "step", "total ms", "share", "ns a write");
    for (name, time) in [("sampling", sampling), ("computing writes", computing), ("applying", applying), ("the tick", total)] {
        println!("{name:<20} {:>12.1} {:>9.1}% {:>14.1}", time.as_secs_f64() * 1e3, 100.0 * time.as_secs_f64() / total.as_secs_f64(), time.as_nanos() as f64 / writes as f64);
    }
    println!("writes a second, one core: {:.2} million; ticks a second: {:.0}", writes as f64 / total.as_secs_f64() / 1e6, ticks as f64 / total.as_secs_f64());
}
