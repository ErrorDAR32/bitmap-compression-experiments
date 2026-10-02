//! Frames of grass over the mock superchunk, for a video: one frame
//! every `every` ticks -- a cell a pixel, dirt brown, grass green --
//! written to standard output as raw RGB, 1024x1024, for ffmpeg:
//!
//! `cargo run --release --example video -- [ticks] [grass cells at the start] [ticks a frame] | ffmpeg -f rawvideo -pix_fmt rgb24 -s 1024x1024 -r 30 -i - grass.mp4`

use bitmap::morton::morton_coordinates;
use bitmap::BITS_PER_WORD;
use bitplane_manager::{BitmapArena, BucketKey};
use chunk_storage::mock::{grass_on_dirt, DIRT, GRASS};
use chunk_storage::{ChunkPlace, ChunkPosition, ChunkStorage, LayerCodec, SuperChunkPosition, CHUNK_SIDE, SUPERCHUNK_SIDE_CELLS};
use std::io::Write;
use tilesim::grass;

/// Dirt's colour.
const BROWN: [u8; 3] = [116, 80, 46];
/// Grass's colour.
const GREEN: [u8; 3] = [72, 160, 56];

/// The superchunk's cells as RGB pixels, row by row: brown, green where
/// grass holds.
fn frame(arena: &BitmapArena, superchunk: SuperChunkPosition, pixels: &mut [u8]) {
    let side = SUPERCHUNK_SIDE_CELLS as usize;
    for pixel in pixels.chunks_exact_mut(3) {
        pixel.copy_from_slice(&BROWN);
    }
    for place in ChunkPlace::all() {
        let bucket = arena.bucket(BucketKey { layer_type: GRASS, chunk: ChunkPosition::of(superchunk, place) }).expect("hot");
        for (word_index, &word) in bucket.cells().iter().enumerate() {
            let mut bits = word;
            while bits != 0 {
                let (x, y) = morton_coordinates(word_index * BITS_PER_WORD + bits.trailing_zeros() as usize);
                let (x, y) = (place.x() as usize * CHUNK_SIDE + x as usize, place.y() as usize * CHUNK_SIDE + y as usize);
                pixels[(y * side + x) * 3..][..3].copy_from_slice(&GREEN);
                bits &= bits - 1;
            }
        }
    }
}

/// Ticks the grass, writing a frame every so many ticks.
fn main() {
    let arguments: Vec<usize> = std::env::args().skip(1).map(|argument| argument.parse().expect("a number")).collect();
    let ticks = arguments.first().copied().unwrap_or(120_000);
    let grass_cells = arguments.get(1).copied().unwrap_or(2000);
    let every = arguments.get(2).copied().unwrap_or(256);

    let superchunk = SuperChunkPosition { x: 1 << 21, y: 1 << 21 };
    let (mut codec, mut arena, mut storage) = (LayerCodec::new(), BitmapArena::new(), ChunkStorage::new(1 << 16));
    storage.insert(superchunk, grass_on_dirt(1, grass_cells, &mut codec));
    for place in ChunkPlace::all() {
        arena.make_hot_layers(ChunkPosition::of(superchunk, place), &[DIRT, GRASS], &storage, &mut codec);
    }

    let side = SUPERCHUNK_SIDE_CELLS as usize;
    let mut pixels = vec![0u8; side * side * 3];
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for tick in 0..=ticks {
        if tick % every == 0 {
            frame(&arena, superchunk, &mut pixels);
            out.write_all(&pixels).expect("standard output");
            if tick % (every * 50) == 0 {
                eprintln!("tick {tick:>7}: grass {}", arena.superchunk_count(GRASS, superchunk));
            }
        }
        grass::tick(&mut arena, 1, tick as u64);
    }
}
