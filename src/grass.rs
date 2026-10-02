//! Grass over dirt: TileSim's first rule. Each tick every cell of grass
//! may spread onto a dirt neighbour, or decay back to dirt the more grass
//! is around it:
//!
//! - **Spreading**: a cell of grass tries to spread with
//!   [`SPREAD_CHANCE`], onto one of its eight neighbours drawn at
//!   random, if that one is dirt.
//! - **Decay**: a cell of grass with `k` of its eight neighbours grass
//!   turns back to dirt with `k / 8` of [`DECAY_CHANCE`]: none with no
//!   grass around, the whole chance with grass all round.
//!
//! One sampling pass serves both, and no sample is wasted: every cell of
//! grass is sampled with the two chances together, and each sample
//! draws one neighbour and which of the two it tries -- spreading, in
//! [`SPREAD_CHANCE`] of the sum, else decay. Decay so happens when the
//! neighbour drawn is grass: `k / 8` of the time, as asked, from one
//! neighbour read rather than eight.
//!
//! Writes are queued as the samples come, in Morton order, and applied
//! at the tick's end: every sample reads the world as the tick found it.
//! The two never touch one cell in a tick: decay clears cells that were
//! grass, spreading fills cells that were dirt.

use bitplane_manager::{BitmapArena, Random, Write, WriteOp};
use chunk_storage::mock::{DIRT, GRASS};
use chunk_storage::WorldCell;

/// The chance, each tick, that a cell of grass tries to spread.
pub const SPREAD_CHANCE: f64 = 0.001;
/// The chance, each tick, that a cell of grass with grass all round
/// turns back to dirt.
pub const DECAY_CHANCE: f64 = 0.002;

/// A cell's eight neighbours, as offsets.
const NEIGHBOURS: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

/// What a tick did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tick {
    /// Cells of grass sampled.
    pub sampled: usize,
    /// Writes queued: two a spread or a decay.
    pub writes: usize,
    /// Spreads queued: two samples may spread onto one cell, which then
    /// changes once.
    pub spreads: usize,
    /// Cells of grass turned back to dirt.
    pub decays: usize,
}

/// One tick of grass over the hot bitplanes: [`sample`], [`compute`],
/// then the writes applied. `samples` is room for the cells sampled,
/// kept between ticks so a tick allocates nothing once it has grown.
pub fn tick(arena: &mut BitmapArena, random: &mut Random, samples: &mut Vec<WorldCell>) -> Tick {
    let sampled = sample(arena, random, samples);
    let (spreads, decays) = compute(arena, random, samples);
    arena.apply();
    Tick { sampled, writes: 2 * (spreads + decays), spreads, decays }
}

/// The tick's first step: every cell of grass chosen with the chances of
/// spreading and of decay together, into `samples`, in Morton order:
/// how many.
pub fn sample(arena: &BitmapArena, random: &mut Random, samples: &mut Vec<WorldCell>) -> usize {
    samples.clear();
    arena.sample(GRASS, SPREAD_CHANCE + DECAY_CHANCE, random, |cell| samples.push(cell))
}

/// The tick's second step: each sampled cell draws a neighbour, and
/// whether it tries to spread or to decay, and queues the writes if the
/// neighbour lets it: how many spreads and decays were queued.
pub fn compute(arena: &mut BitmapArena, random: &mut Random, samples: &[WorldCell]) -> (usize, usize) {
    let spread_share = SPREAD_CHANCE / (SPREAD_CHANCE + DECAY_CHANCE);
    let (mut spreads, mut decays) = (0, 0);
    for &cell in samples {
        let (dx, dy) = NEIGHBOURS[random.below(NEIGHBOURS.len() as u32) as usize];
        let spreading = random.unit() <= spread_share;
        let (Some(x), Some(y)) = (cell.x.checked_add_signed(dx), cell.y.checked_add_signed(dy)) else {
            continue;
        };
        let neighbour = WorldCell { x, y };
        if spreading {
            if arena.holds(DIRT, neighbour) == Ok(true) {
                arena.queue(GRASS, Write::cell(neighbour, WriteOp::Set));
                arena.queue(DIRT, Write::cell(neighbour, WriteOp::Unset));
                spreads += 1;
            }
        } else if arena.holds(GRASS, neighbour) == Ok(true) {
            arena.queue(GRASS, Write::cell(cell, WriteOp::Unset));
            arena.queue(DIRT, Write::cell(cell, WriteOp::Set));
            decays += 1;
        }
    }
    (spreads, decays)
}
