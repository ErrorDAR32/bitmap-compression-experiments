//! Runs grass spreading over the mock superchunk -- dirt, with a few
//! cells of grass -- and prints how the grass grows.
//!
//! `cargo run --release -- [ticks] [grass cells at the start]`

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

use bitplane_manager::BitmapArena;
use chunk_storage::mock::{grass_on_dirt, DIRT, GRASS};
use chunk_storage::{ChunkPlace, ChunkPosition, ChunkStorage, LayerCodec, SuperChunkPosition, WORLD_SIDE_SUPERCHUNKS};
use std::time::Instant;
use tilesim::grass;

/// Runs the ticks asked for, printing the grass every tenth of them.
fn main() {
    let arguments: Vec<usize> = std::env::args().skip(1).map(|argument| argument.parse().expect("a number")).collect();
    let ticks = arguments.first().copied().unwrap_or(10_000);
    let grass_cells = arguments.get(1).copied().unwrap_or(64);

    let superchunk = SuperChunkPosition { x: WORLD_SIDE_SUPERCHUNKS / 2, y: WORLD_SIDE_SUPERCHUNKS / 2 };
    let (mut codec, mut arena, mut storage) = (LayerCodec::new(), BitmapArena::new(), ChunkStorage::new(1 << 16));
    storage.insert(superchunk, grass_on_dirt(1, grass_cells, &mut codec));
    for place in ChunkPlace::all() {
        arena.make_hot_layers(ChunkPosition::of(superchunk, place), &[DIRT, GRASS], &storage, &mut codec);
    }

    println!("{:>8} {:>10} {:>10} {:>12}", "tick", "grass", "dirt", "µs a tick");
    let start = Instant::now();
    let report_every = (ticks / 10).max(1);
    for tick in 1..=ticks {
        grass::tick(&mut arena, 1, tick as u64);
        if tick % report_every == 0 {
            let micros = start.elapsed().as_secs_f64() * 1e6 / tick as f64;
            println!("{tick:>8} {:>10} {:>10} {micros:>12.2}", arena.superchunk_count(GRASS, superchunk), arena.superchunk_count(DIRT, superchunk));
        }
    }
}
