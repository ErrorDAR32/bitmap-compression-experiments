//! Cells into pixels, on a thread of its own: between the simulation,
//! which only copies the cells in view, and the window, which only
//! shows pixels. So drawing takes no time from the ticks, however much
//! of the world is in view, and none from the window's frames.
//!
//! A cell is a pixel in one solid colour: dirt brown, grass green, a
//! sheep white -- a few pixels across, to be seen.

use crate::sim::{Cells, Frame, CHUNK_WORDS};
use bitmap::morton::morton_coordinates;
use bitmap::BITS_PER_WORD;
use coordinates::{ChunkPlace, CHUNK_SIDE, SUPERCHUNK_SIDE_CELLS};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::Instant;
use tilesim::diagnostics::frames::{BROWN, GREEN, WHITE};

/// Pixels along a superchunk's side: a cell each.
const SIDE: usize = SUPERCHUNK_SIDE_CELLS as usize;

/// Cells from a sheep's own its square is drawn out to, each way.
const SHEEP_REACH: usize = 1;

/// One superchunk's pixels.
pub struct Tile {
    /// Where it is in the world's square, `(x, y)` from the top left.
    pub at: (u32, u32),
    /// Its cells, a pixel each, row by row: red, green, blue, opacity.
    pub pixels: Vec<u8>,
}

/// A frame, painted.
pub struct Picture {
    /// Ticks run so far.
    pub tick: u64,
    /// Ticks a second, over the time since the frame before.
    pub ticks_a_second: f64,
    /// Sheep in the whole world.
    pub sheep: usize,
    /// Cells of grass in the whole world.
    pub grass: u64,
    /// What answering took of the simulation's thread, in seconds.
    pub sync_seconds: f64,
    /// The share of the thread's time that is.
    pub sync_share: f64,
    /// What painting took of the painter's thread, in seconds.
    pub paint_seconds: f64,
    /// The superchunks asked for.
    pub tiles: Vec<Tile>,
}

/// Starts the painter's thread: every frame from `frames` painted, and
/// sent on. It stops once either end is dropped.
pub fn start(frames: Receiver<Frame>) -> Receiver<Picture> {
    let (painted, pictures) = channel();
    thread::Builder::new()
        .name("painter".to_string())
        .spawn(move || {
            for frame in frames {
                let started = Instant::now();
                let tiles = frame.cells.iter().map(paint).collect();
                let picture = Picture {
                    tick: frame.tick,
                    ticks_a_second: frame.ticks_a_second,
                    sheep: frame.sheep,
                    grass: frame.grass,
                    sync_seconds: frame.sync_seconds,
                    sync_share: frame.sync_share,
                    paint_seconds: started.elapsed().as_secs_f64(),
                    tiles,
                };
                if painted.send(picture).is_err() {
                    return;
                }
            }
        })
        .expect("a thread for the painter");
    pictures
}

/// `colour`, opaque.
const fn opaque(colour: [u8; 3]) -> [u8; 4] {
    [colour[0], colour[1], colour[2], u8::MAX]
}

/// A superchunk's cells as pixels: dirt, its grass over it, its sheep
/// over that.
fn paint(cells: &Cells) -> Tile {
    let mut pixels = vec![opaque(BROWN); SIDE * SIDE];
    let green = opaque(GREEN);
    for (place, chunk) in ChunkPlace::all().zip(cells.grass.as_chunks::<CHUNK_WORDS>().0) {
        let (left, top) = (place.x() as usize * CHUNK_SIDE, place.y() as usize * CHUNK_SIDE);
        for (word_index, &word) in chunk.iter().enumerate() {
            let mut bits = word;
            while bits != 0 {
                let (x, y) = morton_coordinates(word_index * BITS_PER_WORD + bits.trailing_zeros() as usize);
                pixels[(top + y as usize) * SIDE + left + x as usize] = green;
                bits &= bits - 1;
            }
        }
    }
    let white = opaque(WHITE);
    for &(x, y) in &cells.sheep {
        let (x, y) = (x as usize, y as usize);
        for y in y.saturating_sub(SHEEP_REACH)..=(y + SHEEP_REACH).min(SIDE - 1) {
            for x in x.saturating_sub(SHEEP_REACH)..=(x + SHEEP_REACH).min(SIDE - 1) {
                pixels[y * SIDE + x] = white;
            }
        }
    }
    Tile { at: cells.at, pixels: pixels.into_flattened() }
}
