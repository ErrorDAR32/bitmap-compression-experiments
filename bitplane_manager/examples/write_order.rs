//! Does applying writes in Morton order pay? The same random writes,
//! applied as drawn and sorted by the Morton index of each shape's
//! anchor cell, timed over many runs on mock superchunks of dirt and
//! grass, every chunk of both hot.
//!
//! `cargo run --release --example write_order -- [writes a run] [runs] [superchunks]`
//!
//! Only `apply` is timed, and the sort on its own. Each run draws new
//! writes and applies them both ways, the order of the two alternating
//! from run to run so neither always meets the caches the other left.

use bitplane_manager::{BitmapArena, Shape, Write, WriteOp};
use chunk_storage::mock::{grass_on_dirt, DIRT, GRASS};
use chunk_storage::{ChunkPlace, ChunkPosition, ChunkStorage, LayerCodec, SuperChunkPosition, WorldCell, SUPERCHUNK_SIDE_CELLS, WORLD_SIDE_SUPERCHUNKS};
use std::time::{Duration, Instant};

/// A xorshift64* generator: enough for drawing writes.
struct Random(u64);

impl Random {
    /// The next 64 random bits.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number below `bound`.
    fn below(&mut self, bound: u32) -> u32 {
        (((self.next() >> 32) * bound as u64) >> 32) as u32
    }
}

/// The superchunks used: a square of them, row by row, from the world's
/// middle.
fn superchunks(count: u32) -> Vec<SuperChunkPosition> {
    let side = (count as f64).sqrt().ceil() as u32;
    let middle = WORLD_SIDE_SUPERCHUNKS / 2;
    (0..count).map(|index| SuperChunkPosition { x: middle + index % side, y: middle + index / side }).collect()
}

/// `count` random writes over `superchunks`: nine in ten a single cell,
/// the rest rectangles up to 8x8 and discs up to radius 4, each set,
/// unset or flip, on dirt or grass.
fn draw_writes(random: &mut Random, superchunks: &[SuperChunkPosition], count: usize) -> Vec<Write> {
    (0..count)
        .map(|_| {
            let superchunk = superchunks[random.below(superchunks.len() as u32) as usize];
            let cell = WorldCell {
                x: superchunk.x * SUPERCHUNK_SIDE_CELLS + random.below(SUPERCHUNK_SIDE_CELLS),
                y: superchunk.y * SUPERCHUNK_SIDE_CELLS + random.below(SUPERCHUNK_SIDE_CELLS),
            };
            let shape = match random.below(20) {
                0 => Shape::Rect { corner: cell, width: 1 + random.below(8), height: 1 + random.below(8) },
                1 => Shape::Disc { center: cell, radius: random.below(5) },
                _ => Shape::Cell(cell),
            };
            let op = [WriteOp::Set, WriteOp::Unset, WriteOp::Flip][random.below(3) as usize];
            Write { layer_type: if random.below(2) == 0 { DIRT } else { GRASS }, op, shape }
        })
        .collect()
}

/// The cell a write's shape is anchored at: its cell, corner or centre.
fn anchor(write: &Write) -> WorldCell {
    match write.shape {
        Shape::Cell(cell) | Shape::Rect { corner: cell, .. } | Shape::Disc { center: cell, .. } => cell,
    }
}

/// Queues `writes` and times applying them: the time, and the cells
/// missed -- of shapes spilling past the superchunks used, whose
/// neighbours are not hot.
fn timed_apply(arena: &mut BitmapArena, writes: &[Write]) -> (Duration, u64) {
    writes.iter().for_each(|&write| arena.queue(write));
    let start = Instant::now();
    let applied = arena.apply();
    (start.elapsed(), applied.missed)
}

/// The mean, median and least of `times`, in nanoseconds a write.
fn summary(times: &mut [Duration], writes: usize) -> (f64, f64, f64) {
    times.sort();
    let per_write = |time: Duration| time.as_nanos() as f64 / writes as f64;
    let mean = times.iter().map(|&time| per_write(time)).sum::<f64>() / times.len() as f64;
    (mean, per_write(times[times.len() / 2]), per_write(times[0]))
}

fn main() {
    let arguments: Vec<usize> = std::env::args().skip(1).map(|argument| argument.parse().expect("a number")).collect();
    let writes_a_run = arguments.first().copied().unwrap_or(100_000);
    let runs = arguments.get(1).copied().unwrap_or(50);
    let superchunk_count = arguments.get(2).copied().unwrap_or(1) as u32;

    let (mut codec, mut arena, mut storage) = (LayerCodec::new(), BitmapArena::new(), ChunkStorage::new(1 << 16));
    let superchunks = superchunks(superchunk_count);
    for (seed, &superchunk) in superchunks.iter().enumerate() {
        storage.insert(superchunk, grass_on_dirt(seed as u64 + 1, 64, &mut codec));
        for place in ChunkPlace::all() {
            arena.make_hot_layers(ChunkPosition::of(superchunk, place), &[DIRT, GRASS], &storage, &mut codec);
        }
    }

    let mut random = Random(0x9E37_79B9_7F4A_7C15);
    let (mut unordered, mut ordered, mut sorting, mut missed) = (Vec::new(), Vec::new(), Vec::new(), 0);
    for run in 0..runs {
        let writes = draw_writes(&mut random, &superchunks, writes_a_run);
        let start = Instant::now();
        // Each key computed once; ties keep the order drawn, so the
        // latest of two writes at one anchor still wins.
        let mut keys: Vec<(u64, u32)> = writes.iter().enumerate().map(|(index, write)| (anchor(write).morton_index(), index as u32)).collect();
        keys.sort_unstable();
        let sorted: Vec<Write> = keys.iter().map(|&(_, index)| writes[index as usize]).collect();
        sorting.push(start.elapsed());
        let mut apply = |times: &mut Vec<Duration>, writes: &[Write]| {
            let (time, cells) = timed_apply(&mut arena, writes);
            times.push(time);
            missed += cells;
        };
        if run % 2 == 0 {
            apply(&mut unordered, &writes);
            apply(&mut ordered, &sorted);
        } else {
            apply(&mut ordered, &sorted);
            apply(&mut unordered, &writes);
        }
    }

    println!("{writes_a_run} writes a run, {runs} runs, {superchunk_count} superchunk(s) of dirt and grass, every chunk hot");
    println!("{:<22} {:>10} {:>10} {:>10}", "ns a write", "mean", "median", "least");
    for (name, times) in [("apply, as drawn", &mut unordered), ("apply, Morton order", &mut ordered), ("the sort alone", &mut sorting)] {
        let (mean, median, least) = summary(times, writes_a_run);
        println!("{name:<22} {mean:>10.1} {median:>10.1} {least:>10.1}");
    }
    println!("cells missed, past the superchunks used: {missed}");
}
