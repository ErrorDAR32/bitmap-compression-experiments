//! Sheep on grass: they eat it, starve without it, breed lambs that
//! grow up -- attributes coming and going -- walk to the nearest grass
//! when hungry, never stand two on a cell, never walk off the bitplanes held, and tick the same on
//! any number of threads.
//!
//! `cargo test`

use bitplane_manager::{Write, WriteOp};
use chunk_storage::mock::{DIRT, GRASS};
use coordinates::{CartesianCell, SUPERCHUNK_SIDE_CELLS};
use simulation::entities::{Attribute, EntityId, EntityRef, Header, OCCUPIED};
use simulation::Simulation;
use tilesim::diagnostics::world::World;
use tilesim::pasture::tick;
use tilesim::sheep::{rule, SheepTickMetrics, HUNGER, LAMB, MEAL_WAKES, PREGNANT, SHEEP, STARVE_WAKES, STEP_JITTER, STEP_TICKS};

/// Every sheep is hungry no longer than it starves at, is a sheep, and is
/// never both a lamb and pregnant.
fn well_formed(sheep: EntityRef) {
    assert_eq!(sheep.header.kind, SHEEP);
    assert!(sheep.attribute(HUNGER).expect("hunger, always") < STARVE_WAKES);
    assert!(sheep.attribute(LAMB).is_none() || sheep.attribute(PREGNANT).is_none(), "a lamb, pregnant");
}

/// With no grass at all, every sheep starves, within the wakes it
/// starves at.
#[test]
fn sheep_without_grass_starve() {
    let mut world = World::with_sheep(1, 0, 500);
    let mut simulation = Simulation::new(1);
    let (mut eaten, mut deaths) = (0, 0);
    for seed in 0..STARVE_WAKES * (STEP_TICKS + STEP_JITTER) + STEP_TICKS {
        let done = tick(&mut simulation, &mut world.arena, &mut world.entities, seed).rules.sheep;
        (eaten, deaths) = (eaten + done.eaten, deaths + done.deaths);
    }
    assert_eq!((eaten, deaths, world.sheep()), (0, 500, 0));
}

/// On grass, sheep eat -- each cell eaten turned to dirt -- breed, and
/// their lambs grow up: pregnancy and youth added and removed as they
/// go, and no sheep walks off the superchunks held.
#[test]
fn sheep_eat_breed_and_grow_up() {
    let mut world = World::with_sheep(4, 300_000, 400);
    let mut simulation = Simulation::new(2);
    let (mut eaten, mut births, mut lost, mut lambs_seen, mut pregnant_seen) = (0, 0, 0, false, false);
    for seed in 0..40_000 {
        let report = tick(&mut simulation, &mut world.arena, &mut world.entities, seed);
        (eaten, births, lost) = (eaten + report.rules.sheep.eaten, births + report.rules.sheep.births, lost + report.entities.lost);
        if seed % 500 == 0 {
            for sheep in world.entities.iter() {
                well_formed(sheep);
                lambs_seen |= sheep.attribute(LAMB).is_some();
                pregnant_seen |= sheep.attribute(PREGNANT).is_some();
            }
        }
    }
    assert!(eaten > 5_000 && births > 100, "{eaten} eaten, {births} born");
    assert!(lambs_seen && pregnant_seen);
    assert_eq!(lost, 0, "no sheep walks off the superchunks held");
    assert!(world.entities.iter().any(|sheep| sheep.attribute(LAMB).is_none() && sheep.attribute(HUNGER).is_some()), "grown sheep");
}

/// A hungry sheep with no grass beside it walks the shortest way to the
/// nearest in the area about it: one eight cells from the only grass
/// stands on it eight wakes on, eats it, and no path was looked for
/// that was not found.
#[test]
fn hungry_sheep_walk_to_the_nearest_grass() {
    let mut world = World::grass_on_dirt(1, 0);
    let superchunk = world.superchunks[0];
    let corner = CartesianCell { x: superchunk.x * SUPERCHUNK_SIDE_CELLS, y: superchunk.y * SUPERCHUNK_SIDE_CELLS };
    let (sheep, grass) = (CartesianCell { x: corner.x + 500, y: corner.y + 500 }, CartesianCell { x: corner.x + 506, y: corner.y + 493 });
    world.arena.queue(GRASS, Write::cell(grass.into(), WriteOp::Set));
    world.arena.queue(DIRT, Write::cell(grass.into(), WriteOp::Unset));
    world.arena.apply();
    let header = Header { id: EntityId(1), kind: SHEEP, at: sheep.into(), wake: 0 };
    world.entities.queue_put(header, &[Attribute { kind: HUNGER, value: MEAL_WAKES }]);
    world.entities.apply(&mut world.arena);
    let mut simulation = Simulation::new(1);
    let (mut done, mut ate_at) = (SheepTickMetrics::default(), None);
    for seed in 0..12 * (STEP_TICKS + STEP_JITTER) {
        // The grass rule left out: the one cell of grass must stay until eaten.
        let report = simulation.tick(&mut world.arena, &mut world.entities, seed, |turn, _| rule(turn));
        if report.rules.eaten > 0 && ate_at.is_none() {
            ate_at = Some(done.woken);
        }
        done += report.rules;
    }
    assert_eq!(done.eaten, 1, "the one cell of grass, eaten");
    assert_eq!(ate_at, Some(7), "seven steps to it, eaten on the wake after");
    assert_eq!((done.sought, done.paths), (6, 6), "a path found each step until the grass was beside it");
    assert_eq!(world.grass(), 0);
}

/// Grass and sheep over four superchunks, across their borders, come out
/// the same on one thread and on four.
#[test]
fn any_number_of_threads_ticks_sheep_the_same() {
    let run = |threads| {
        let mut world = World::with_sheep(4, 200_000, 300);
        let mut simulation = Simulation::new(threads);
        let reports: Vec<_> = (0..1500).map(|seed| tick(&mut simulation, &mut world.arena, &mut world.entities, seed)).map(|report| (report.rules, report.entities)).collect();
        let sheep: Vec<_> = world.entities.iter().map(|sheep| (sheep.header, sheep.attributes.to_vec())).collect();
        (reports, sheep, world.grass())
    };
    let (one, four) = (run(1), run(4));
    assert!(!one.1.is_empty());
    assert_eq!(one, four);
}

/// Sheep never overlap: a crowded flock, eating, breeding and walking
/// over four superchunks and their borders, stands one to a cell after
/// every tick checked, and the bitplane of the cells entities stand on
/// says the same.
#[test]
fn sheep_never_overlap() {
    let mut world = World::with_sheep(4, 300_000, 60_000);
    assert_eq!(world.sheep(), 240_000, "each on a cell of its own from the start");
    let mut simulation = Simulation::new(4);
    let (mut stayed, mut births) = (0, 0);
    for seed in 0..2_000 {
        let report = tick(&mut simulation, &mut world.arena, &mut world.entities, seed);
        (stayed, births) = (stayed + report.entities.stayed, births + report.rules.sheep.births);
        if seed % 100 == 99 {
            let mut cells: Vec<_> = world.entities.iter().map(|sheep| sheep.header.at).collect();
            cells.sort_unstable();
            assert!(cells.windows(2).all(|pair| pair[0] != pair[1]), "tick {seed}: two sheep on a cell");
            let occupied: u64 = world.superchunks.iter().map(|&superchunk| world.arena.superchunk_count(OCCUPIED, superchunk) as u64).sum();
            assert_eq!(occupied as usize, cells.len(), "tick {seed}: the cells occupied are the cells stood on");
        }
    }
    assert!(stayed > 1_000, "{stayed} sheep found their cell taken, and stayed");
    assert!(births > 0);
}
