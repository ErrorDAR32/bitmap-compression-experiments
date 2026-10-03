//! The simulation's side: a pasture ticked on a thread of its own, which
//! the window asks -- never the other way round -- for the cells in
//! view.
//!
//! The window sends [`Request`]s; the simulation reads them between
//! ticks, and answers each [`Request::Sync`] with a [`Frame`]: the
//! superchunks in view as the last tick left them -- their grass's
//! words, copied as they are, and where their sheep stand. It copies and
//! nothing more: turning cells into pixels is [`crate::paint`]'s, on
//! another thread, so what is in view costs the ticks next to nothing
//! -- and a frame carries only so many superchunks, the window going
//! round those in view, so it costs no more however many there are.
//! It sends nothing unasked, so it is the window that sets how often
//! the world is drawn, and a window that falls behind slows no tick.
//!
//! It ticks until the window is closed, with no number of ticks to
//! stop at, and keeps a census of the flock and the grass as it goes
//! ([`census_path`]): what a long run came to is there once it is
//! closed.

use bitplane_manager::BucketKey;
use chunk_storage::mock::GRASS;
use chunk_storage::LayerType;
use terrain::{WALL_EAST, WALL_SOUTH};
use coordinates::{ChunkPlace, ChunkPosition, SuperchunkPosition, CHUNKS_IN_SUPERCHUNK, SUPERCHUNK_SIDE_CELLS, WORLD_SIDE_SUPERCHUNKS};
use simulation::Simulation;
use std::fs::{create_dir_all, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};
use entity_rules::diagnostics::world::MockWorld;

/// Ticks a second the simulation is held to unless told otherwise: the
/// game's target.
pub const TARGET_PACE: u32 = 256;

/// The seed of the world watched.
pub const SEED: u64 = 1;

/// Ticks from one line of the census to the next.
pub const CENSUS_EVERY: u64 = 1000;

/// Where the census of the run is kept: the flock and the grass every
/// [`CENSUS_EVERY`] ticks, written as the run goes, so a run closed at
/// any time leaves what it came to. Under the crate's folder, out of
/// git, as every crate's transient data.
pub fn census_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("transient_data/measurements/census.csv")
}

/// Starts the census afresh: its file, with what was run and the
/// columns' names. `None`, and no census kept, if it cannot be made.
fn census(superchunks: u32, thousandths: usize, flock: usize) -> Option<BufWriter<File>> {
    let path = census_path();
    create_dir_all(path.parent()?).ok()?;
    let mut file = BufWriter::new(File::create(path).ok()?);
    writeln!(file, "# viewer {superchunks} {thousandths} {flock}").ok()?;
    writeln!(file, "tick,sheep,grass").ok()?;
    Some(file)
}

/// Words a chunk's bitmap takes.
pub const CHUNK_WORDS: usize = bitmap::WORDS;

/// The superchunks in view: a rectangle of them, counted from the top
/// left of the world's square, both corners in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    /// The top left superchunk, `(x, y)`.
    pub first: (u32, u32),
    /// The bottom right one.
    pub last: (u32, u32),
}

/// What the window wants of the world, one frame: some of the
/// superchunks in view -- as many as a frame may carry, the window
/// going round them frame after frame -- and how finely it will draw
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ask {
    /// The superchunks in view.
    pub viewport: Viewport,
    /// How coarsely they are drawn: a pixel `2^detail` cells a side.
    /// Passed on to the painter; the simulation copies the same.
    pub detail: u32,
    /// Superchunks of the view, row by row, to pass over first.
    pub skip: u32,
    /// Superchunks to answer with, at most.
    pub most: u32,
}

/// What the window asks of the simulation.
#[derive(Clone, Copy, Debug)]
pub enum Request {
    /// Some of the superchunks in view: answered with a [`Frame`].
    Sync(Ask),
    /// Stop ticking, or go on.
    Pause(bool),
    /// Tick so many times a second, or flat out.
    Pace(Option<u32>),
}

/// One superchunk's cells, as a tick left them.
pub struct Cells {
    /// Where it is in the world's square, `(x, y)` from the top left.
    pub at: (u32, u32),
    /// Its grass: its 16 chunks' bitmaps one after another, in the
    /// chunks' Morton order, [`CHUNK_WORDS`] words each, in Morton order
    /// -- as the arena holds them. A chunk not hot is all clear.
    pub grass: Vec<u64>,
    /// Its cliffs, laid out as the grass: the cells keeping a wall to
    /// their east or south.
    pub cliffs: Vec<u64>,
    /// The cells its sheep stand on, `(x, y)` from its top left.
    pub sheep: Vec<(u16, u16)>,
}

/// The world in view, as a tick left it.
pub struct Frame {
    /// Ticks run so far.
    pub tick: u64,
    /// Ticks a second, over the time since the frame before.
    pub ticks_a_second: f64,
    /// Sheep in the whole world.
    pub sheep: usize,
    /// Cells of grass in the whole world.
    pub grass: u64,
    /// What answering took of the simulation's thread -- the counts and
    /// the copy, all the window costs it -- in seconds.
    pub sync_seconds: f64,
    /// The share of the thread's time that is, at the rate asked.
    pub sync_share: f64,
    /// How coarsely the window will draw them ([`Ask::detail`]).
    pub detail: u32,
    /// The superchunks asked for.
    pub cells: Vec<Cells>,
}

/// Superchunks along the side of the square `superchunks` of them make.
pub fn side(superchunks: u32) -> u32 {
    (superchunks as f64).sqrt().ceil() as u32
}

/// Starts a pasture of `superchunks` superchunks -- grass on
/// `thousandths` of the cells, `flock` sheep on each -- ticking on
/// every thread the machine has, on a thread of its own: where to send it requests,
/// and where its frames come back. It stops once the requests' sender is
/// dropped.
pub fn start(superchunks: u32, thousandths: usize, flock: usize) -> (Sender<Request>, Receiver<Frame>) {
    let (requests, asked) = channel();
    let (answers, frames) = channel();
    thread::Builder::new()
        .name("simulation".to_string())
        .spawn(move || run(superchunks, thousandths, flock, &asked, &answers))
        .expect("a thread for the simulation");
    (requests, frames)
}

/// The simulation's thread: requests read between ticks, a tick, and a
/// wait for the next one's time.
fn run(superchunks: u32, thousandths: usize, flock: usize, asked: &Receiver<Request>, answers: &Sender<Frame>) {
    // A world generated, terrain and all, held as the mock is: the superchunks row by row.
    let made = world::generate_with(SEED, superchunks, (1 << 20) * thousandths / 1000, flock);
    let across = side(superchunks);
    let middle = WORLD_SIDE_SUPERCHUNKS / 2;
    let positions = (0..superchunks).map(|index| SuperchunkPosition { x: middle + index % across, y: middle + index / across }).collect();
    let mut world = MockWorld { arena: made.arena, entities: made.entities, storage: made.storage, superchunks: positions };
    let mut simulation = Simulation::for_superchunks(superchunks as usize);
    let (mut paused, mut pace, mut tick) = (false, Some(TARGET_PACE), 0u64);
    let mut census = census(superchunks, thousandths, flock);
    let (mut next_tick, mut last_frame, mut last_frame_tick) = (Instant::now(), Instant::now(), 0u64);
    loop {
        // Paused, there is nothing to do until the window asks.
        let mut request = if paused { asked.recv().ok() } else { None };
        loop {
            match request.take().map_or_else(|| asked.try_recv(), Ok) {
                Ok(Request::Sync(ask)) => {
                    let asked_at = Instant::now();
                    let elapsed = last_frame.elapsed().as_secs_f64();
                    let ticks_a_second = if elapsed > 0.0 { (tick - last_frame_tick) as f64 / elapsed } else { 0.0 };
                    (last_frame, last_frame_tick) = (asked_at, tick);
                    let (sheep, grass, cells) = (world.entities.len(), world.grass(), copy(&world, superchunks, ask));
                    let sync_seconds = asked_at.elapsed().as_secs_f64();
                    let sync_share = if elapsed > 0.0 { sync_seconds / elapsed } else { 0.0 };
                    let frame = Frame { tick, ticks_a_second, sheep, grass, sync_seconds, sync_share, detail: ask.detail, cells };
                    if answers.send(frame).is_err() {
                        return;
                    }
                }
                Ok(Request::Pause(pause)) => (paused, next_tick) = (pause, Instant::now()),
                Ok(Request::Pace(new)) => (pace, next_tick) = (new, Instant::now()),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        if paused {
            continue;
        }
        if tick.is_multiple_of(CENSUS_EVERY) {
            if let Some(file) = &mut census {
                // A line lost is a line lost: the run goes on.
                _ = writeln!(file, "{tick},{},{}", world.entities.len(), world.grass()).and_then(|()| file.flush());
            }
        }
        world::tick(&mut simulation, &mut world.arena, &mut world.entities, tick);
        tick += 1;
        if let Some(pace) = pace {
            next_tick += Duration::from_secs_f64(1.0 / pace as f64);
            let now = Instant::now();
            if next_tick > now {
                thread::sleep(next_tick - now);
            } else {
                // Behind: no catching up in a burst.
                next_tick = now;
            }
        }
    }
}

/// The superchunks of `world` that `ask` asks for, copied: each one's
/// grass, words as they are, and its sheep's cells.
fn copy(world: &MockWorld, superchunks: u32, ask: Ask) -> Vec<Cells> {
    let side = side(superchunks);
    let (first, last) = (ask.viewport.first, ask.viewport.last);
    let in_view = (first.1..=last.1.min(side - 1)).flat_map(|y| (first.0..=last.0.min(side - 1)).map(move |x| (x, y)));
    let mut copied = Vec::new();
    for (x, y) in in_view.skip(ask.skip as usize).take(ask.most as usize) {
        if let Some(&superchunk) = world.superchunks.get((y * side + x) as usize) {
            let mut cliffs = layer(world, WALL_EAST, superchunk);
            cliffs.iter_mut().zip(layer(world, WALL_SOUTH, superchunk)).for_each(|(east, south)| *east |= south);
            copied.push(Cells { at: (x, y), grass: layer(world, GRASS, superchunk), cliffs, sheep: sheep(world, superchunk) });
        }
    }
    copied
}

/// `superchunk`'s cells of `layer_type`: its chunks' words, one chunk
/// after another.
fn layer(world: &MockWorld, layer_type: LayerType, superchunk: SuperchunkPosition) -> Vec<u64> {
    let mut words = Vec::with_capacity(CHUNKS_IN_SUPERCHUNK * CHUNK_WORDS);
    for place in ChunkPlace::all() {
        match world.arena.bucket(BucketKey { layer_type, chunk: ChunkPosition::of(superchunk, place) }) {
            Some(bucket) => words.extend_from_slice(bucket.cells()),
            None => words.resize(words.len() + CHUNK_WORDS, 0),
        }
    }
    words
}

/// The cells `superchunk`'s sheep stand on, from its top left.
fn sheep(world: &MockWorld, superchunk: SuperchunkPosition) -> Vec<(u16, u16)> {
    let Some(held) = world.entities.superchunk(superchunk.morton_index()) else {
        return Vec::new();
    };
    let (left, top) = (superchunk.x * SUPERCHUNK_SIDE_CELLS, superchunk.y * SUPERCHUNK_SIDE_CELLS);
    let mut cells = Vec::with_capacity(held.len());
    for entity in held.iter() {
        let at = entity.header.at.cartesian();
        cells.push(((at.x - left) as u16, (at.y - top) as u16));
    }
    cells
}
