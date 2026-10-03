//! Grass ticked flat out over a mock world, on as many threads as
//! asked: each phase's time, the samples and writes, and the memory held
//! -- the process's, sampled every tick, and the arena's and storage's
//! own.

use entity_rules::diagnostics::world::MockWorld;
use mc_rules::grass;
use simulation::Simulation;
use bitplane_manager::diagnostics::arena::ArenaStats;
use chunk_storage::diagnostics::storage::StorageStats;
use std::time::Duration;
use utilities::memory::MemoryTrack;

/// What a run did, and what it held.
#[derive(Clone, Debug)]
pub struct Throughput {
    /// Ticks run.
    pub ticks: usize,
    /// Superchunks ticked.
    pub superchunks: usize,
    /// Threads ticked on.
    pub threads: usize,
    /// Cells of grass at the start, and at the end.
    pub grass: (u64, u64),
    /// Cells of grass sampled.
    pub sampled: usize,
    /// Writes applied: a write landing in two superchunks counted in each.
    pub writes: usize,
    /// Cells written past the superchunks used.
    pub missed: u64,
    /// The first phase's time, added up: sampling and computing.
    pub computing: Duration,
    /// The second phase's time, added up: applying.
    pub applying: Duration,
    /// The process's memory, sampled after every tick.
    pub memory: MemoryTrack,
    /// What the arena held at the end.
    pub arena: ArenaStats,
    /// What storage held at the end.
    pub storage: StorageStats,
}

/// Ticks grass `ticks` times over `superchunks` superchunks, grass drawn
/// on `thousandths` of each one's cells, on `threads` threads.
pub fn run(ticks: usize, thousandths: usize, superchunks: u32, threads: usize) -> Throughput {
    let mut memory = MemoryTrack::default();
    memory.read();
    let mut world = MockWorld::grass_on_dirt(superchunks, (1 << 20) * thousandths / 1000);
    let start_grass = world.grass();
    let (mut computing, mut applying, mut writes, mut sampled, mut missed) = (Duration::ZERO, Duration::ZERO, 0, 0, 0);
    let mut simulation = Simulation::new(threads);
    for tick in 0..ticks {
        let report = grass::tick(&mut simulation, &mut world.arena, &mut world.entities, tick as u64);
        computing += report.computing;
        applying += report.applying;
        writes += report.applied.writes;
        missed += report.applied.missed;
        sampled += report.rules.sampled;
        memory.read();
    }
    Throughput {
        ticks,
        superchunks: superchunks as usize,
        threads,
        grass: (start_grass, world.grass()),
        sampled,
        writes,
        missed,
        computing,
        applying,
        memory,
        arena: ArenaStats::of(&world.arena),
        storage: StorageStats::of(&world.storage),
    }
}
