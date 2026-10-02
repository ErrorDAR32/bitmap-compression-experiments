//! Grass spreading over dirt: TileSim's first rule. Each tick every cell
//! of grass is sampled with [`SPREAD_CHANCE`]; a sampled cell looks at
//! one of its eight neighbours, drawn at random, and if it is dirt,
//! grass spreads onto it. Writes are queued as the samples come, in
//! Morton order, and applied at the tick's end: every sample reads the
//! world as the tick found it.

use bitplane_manager::{BitmapArena, Random, Shape, Write, WriteOp};
use chunk_storage::mock::{DIRT, GRASS};
use chunk_storage::WorldCell;

/// The chance, each tick, that a cell of grass tries to spread.
pub const SPREAD_CHANCE: f64 = 0.001;

/// A cell's eight neighbours, as offsets.
const NEIGHBOURS: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

/// What a tick did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tick {
    /// Cells of grass sampled.
    pub sampled: usize,
    /// Cells of dirt grass spread onto: two samples may pick the same
    /// one, which counts once.
    pub spread: u64,
}

/// One tick of grass spreading over the hot bitplanes. `samples` is room
/// for the cells sampled, kept between ticks so a tick allocates
/// nothing once it has grown.
pub fn tick(arena: &mut BitmapArena, random: &mut Random, samples: &mut Vec<WorldCell>) -> Tick {
    samples.clear();
    let sampled = arena.sample(GRASS, SPREAD_CHANCE, random, |cell| samples.push(cell));
    for &cell in samples.iter() {
        let (dx, dy) = NEIGHBOURS[random.below(NEIGHBOURS.len() as u32) as usize];
        let (Some(x), Some(y)) = (cell.x.checked_add_signed(dx), cell.y.checked_add_signed(dy)) else {
            continue;
        };
        let neighbour = WorldCell { x, y };
        if arena.holds(DIRT, neighbour) == Ok(true) {
            arena.queue(Write { layer_type: GRASS, op: WriteOp::Set, shape: Shape::Cell(neighbour) });
            arena.queue(Write { layer_type: DIRT, op: WriteOp::Unset, shape: Shape::Cell(neighbour) });
        }
    }
    // Each cell spread onto changes twice: grass set, dirt cleared.
    Tick { sampled, spread: arena.apply().changed / 2 }
}
