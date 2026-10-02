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
//! The rule runs on each superchunk in a tick's first phase
//! ([`bitplane_manager::BitmapArena::tick`]): its writes are queued as
//! the samples come, in Morton order -- grass spreading over a border
//! into the neighbour's queue -- and applied in the second phase, so
//! every sample reads the world as the tick found it. The two never
//! touch one cell in a tick: decay clears cells that were grass,
//! spreading fills cells that were dirt.

use bitplane_manager::{BitmapArena, SuperChunkTick, TickReport, Write, WriteOp};
use chunk_storage::mock::{DIRT, GRASS};
use coordinates::CellIndex;
use std::ops::AddAssign;

/// The chance, each tick, that a cell of grass tries to spread.
pub const SPREAD_CHANCE: f64 = 0.001;
/// The chance, each tick, that a cell of grass with grass all round
/// turns back to dirt.
pub const DECAY_CHANCE: f64 = 0.002;

/// A cell's eight neighbours, as offsets.
const NEIGHBOURS: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

/// What the rule did in a tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Grass {
    /// Cells of grass sampled.
    pub sampled: usize,
    /// Spreads queued: two samples may spread onto one cell, which then
    /// changes once.
    pub spreads: usize,
    /// Cells of grass turned back to dirt.
    pub decays: usize,
}

impl AddAssign for Grass {
    /// Both added up.
    fn add_assign(&mut self, other: Self) {
        self.sampled += other.sampled;
        self.spreads += other.spreads;
        self.decays += other.decays;
    }
}

/// One tick of grass over every superchunk with a bitmap in use, on
/// `threads` threads, `seed` its random numbers' seed -- a new one a
/// tick.
pub fn tick(arena: &mut BitmapArena, threads: usize, seed: u64) -> TickReport<Grass> {
    arena.tick(threads, seed, rule)
}

/// The rule, on one superchunk's turn: every cell of grass chosen with
/// the chances of spreading and of decay together, in Morton order;
/// each draws a neighbour, and whether it tries to spread or to decay,
/// and queues the writes if the neighbour lets it.
pub fn rule(turn: &mut SuperChunkTick, samples: &mut Vec<CellIndex>) -> Grass {
    let sampled = turn.sample(GRASS, SPREAD_CHANCE + DECAY_CHANCE, samples);
    let spread_share = SPREAD_CHANCE / (SPREAD_CHANCE + DECAY_CHANCE);
    let (mut spreads, mut decays) = (0, 0);
    for &cell in samples.iter() {
        let (dx, dy) = NEIGHBOURS[turn.random().below(NEIGHBOURS.len() as u32) as usize];
        let spreading = turn.random().unit() <= spread_share;
        // Stepped on the Morton index itself: no cartesian coordinates.
        let Some(neighbour) = cell.offset(dx, dy) else {
            continue;
        };
        if spreading {
            if turn.holds(DIRT, neighbour) == Ok(true) {
                turn.queue(GRASS, Write::cell(neighbour, WriteOp::Set));
                turn.queue(DIRT, Write::cell(neighbour, WriteOp::Unset));
                spreads += 1;
            }
        } else if turn.holds(GRASS, neighbour) == Ok(true) {
            turn.queue(GRASS, Write::cell(cell, WriteOp::Unset));
            turn.queue(DIRT, Write::cell(cell, WriteOp::Set));
            decays += 1;
        }
    }
    Grass { sampled, spreads, decays }
}
