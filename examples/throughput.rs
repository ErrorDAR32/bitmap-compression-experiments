//! How many writes a second the tick handles, grass -- spreading and
//! decay -- ticking as fast as it goes on as many threads as asked: each
//! phase timed apart -- computing (sampling, reading, queueing) and
//! applying.
//!
//! `cargo run --release --example throughput -- [ticks] [grass, in thousandths of the cells] [superchunks] [threads]`
//!
//! The grass starts scattered at the share asked, so ticks have samples
//! and writes from the first; it grows or shrinks towards its balance as
//! it runs.

use bitplane_manager::BitmapArena;
use chunk_storage::mock::{grass_on_dirt, DIRT, GRASS};
use chunk_storage::{ChunkPlace, ChunkPosition, ChunkStorage, LayerCodec, SuperChunkPosition, WORLD_SIDE_SUPERCHUNKS};
use std::time::Duration;
use tilesim::grass;

/// Runs the ticks asked for, flat out, and prints what each phase took.
fn main() {
    let arguments: Vec<usize> = std::env::args().skip(1).map(|argument| argument.parse().expect("a number")).collect();
    let ticks = arguments.first().copied().unwrap_or(500);
    let thousandths = arguments.get(1).copied().unwrap_or(333);
    let superchunk_count = arguments.get(2).copied().unwrap_or(16) as u32;
    let threads = arguments.get(3).copied().unwrap_or(1);

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

    let (mut computing, mut applying, mut writes, mut sampled, mut missed) = (Duration::ZERO, Duration::ZERO, 0, 0, 0);
    for tick in 0..ticks {
        let report = grass::tick(&mut arena, threads, tick as u64);
        computing += report.computing;
        applying += report.applying;
        writes += report.applied.writes;
        missed += report.applied.missed;
        sampled += report.rules.sampled;
    }
    let total = computing + applying;

    println!("{ticks} ticks over {superchunk_count} superchunk(s) on {threads} thread(s); grass {grass_at_start} -> {}", grass_now(&arena));
    println!("samples {sampled}, writes {writes} ({:.0} a tick), cells missed past the superchunks used {missed}", writes as f64 / ticks as f64);
    println!("{:<20} {:>12} {:>10} {:>14}", "phase", "total ms", "share", "ns a write");
    for (name, time) in [("computing", computing), ("applying", applying), ("the tick", total)] {
        println!("{name:<20} {:>12.1} {:>9.1}% {:>14.1}", time.as_secs_f64() * 1e3, 100.0 * time.as_secs_f64() / total.as_secs_f64(), time.as_nanos() as f64 / writes as f64);
    }
    println!("writes a second: {:.2} million; ticks a second: {:.0}", writes as f64 / total.as_secs_f64() / 1e6, ticks as f64 / total.as_secs_f64());
}
