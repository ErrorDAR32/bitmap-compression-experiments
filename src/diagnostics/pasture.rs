//! Grass and sheep ticked flat out over a mock world, on as many threads
//! as asked: the flock and the grass over the run, what the sheep did,
//! each phase's time, each rule's time in the first -- added up over
//! the threads -- and the memory held.

use entities::diagnostics::world::World;
use crate::grass;
use crate::pasture::Pasture;
use entities::sheep;
use simulation::diagnostics::entities::EntityStats;
use simulation::entities::EntitiesApplied;
use simulation::Simulation;
use std::ops::AddAssign;
use std::time::{Duration, Instant};
use utilities::memory::MemoryTrack;

/// What a run did, and what it held.
#[derive(Clone, Debug)]
pub struct PastureRun {
    /// Ticks run.
    pub ticks: usize,
    /// Superchunks ticked.
    pub superchunks: usize,
    /// Threads ticked on.
    pub threads: usize,
    /// Sheep at the start, and at the end.
    pub sheep: (usize, usize),
    /// Cells of grass at the start, and at the end.
    pub grass: (u64, u64),
    /// What grass and sheep did, added up.
    pub done: Pasture,
    /// What carrying out the changes to entities did, added up.
    pub entities: EntitiesApplied,
    /// Writes applied.
    pub writes: usize,
    /// The first phase's time, added up.
    pub computing: Duration,
    /// The second phase's time, added up.
    pub applying: Duration,
    /// The grass rule's time, over every thread.
    pub grass_time: Duration,
    /// The sheep rule's time, over every thread.
    pub sheep_time: Duration,
    /// The process's memory, sampled after every tick.
    pub memory: MemoryTrack,
    /// What the entities held at the end.
    pub held: EntityStats,
    /// The flock and the grass over the run, every [`CENSUS_EVERY`]
    /// ticks and at the end.
    pub census: Vec<Census>,
}

/// Ticks between two counts of the flock and the grass.
pub const CENSUS_EVERY: usize = 100;

/// The flock and the grass at a tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Census {
    /// The tick about to run.
    pub tick: usize,
    /// Sheep.
    pub sheep: usize,
    /// Cells of grass.
    pub grass: u64,
    /// Sheep woken since the last count.
    pub woken: usize,
    /// Lambs born since the last count.
    pub births: usize,
    /// Sheep starved since the last count.
    pub deaths: usize,
}

/// What the rules did on a turn, and how long each took.
#[derive(Clone, Copy, Default)]
struct Timed {
    /// What they did.
    done: Pasture,
    /// The grass rule's time.
    grass: Duration,
    /// The sheep rule's time.
    sheep: Duration,
}

impl AddAssign for Timed {
    /// Both added up.
    fn add_assign(&mut self, other: Self) {
        self.done += other.done;
        self.grass += other.grass;
        self.sheep += other.sheep;
    }
}

/// Ticks grass and sheep `ticks` times over `superchunks` superchunks,
/// grass drawn on `thousandths` of each one's cells and `sheep` sheep on
/// each, on `threads` threads.
pub fn run(ticks: usize, thousandths: usize, sheep: usize, superchunks: u32, threads: usize) -> PastureRun {
    let mut memory = MemoryTrack::default();
    memory.sample();
    let mut world = World::with_sheep(superchunks, (1 << 20) * thousandths / 1000, sheep);
    let (start_sheep, start_grass) = (world.sheep(), world.grass());
    let (mut timed, mut entities, mut writes, mut computing, mut applying) = (Timed::default(), EntitiesApplied::default(), 0, Duration::ZERO, Duration::ZERO);
    let mut simulation = Simulation::new(threads);
    let mut census = vec![Census { tick: 0, sheep: start_sheep, grass: start_grass, ..Census::default() }];
    let mut since = sheep::SheepTickMetrics::default();
    for tick in 0..ticks {
        let report = simulation.tick(&mut world.arena, &mut world.entities, tick as u64, |turn, samples| {
            let start = Instant::now();
            let grass = grass::rule(turn, samples);
            let grassed = Instant::now();
            let sheep = sheep::rule(turn);
            Timed { done: Pasture { grass, sheep }, grass: grassed - start, sheep: grassed.elapsed() }
        });
        timed += report.rules;
        since += report.rules.done.sheep;
        if (tick + 1) % CENSUS_EVERY == 0 || tick + 1 == ticks {
            census.push(Census { tick: tick + 1, sheep: world.sheep(), grass: world.grass(), woken: since.woken, births: since.births, deaths: since.deaths });
            since = sheep::SheepTickMetrics::default();
        }
        entities += report.entities;
        writes += report.applied.writes;
        computing += report.computing;
        applying += report.applying;
        memory.sample();
    }
    PastureRun {
        ticks,
        superchunks: superchunks as usize,
        threads,
        sheep: (start_sheep, world.sheep()),
        grass: (start_grass, world.grass()),
        done: timed.done,
        entities,
        writes,
        computing,
        applying,
        grass_time: timed.grass,
        sheep_time: timed.sheep,
        memory,
        held: EntityStats::of(&world.entities),
        census,
    }
}
