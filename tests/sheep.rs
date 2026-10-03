//! Sheep on grass: they eat it, starve without it, breed lambs that
//! grow up -- attributes coming and going -- never walk off the
//! bitplanes held, and tick the same on any number of threads.
//!
//! `cargo test`

use simulation::entities::EntityRef;
use simulation::Simulation;
use tilesim::diagnostics::world::World;
use tilesim::pasture::tick;
use tilesim::sheep::{HUNGER, LAMB, PREGNANT, SHEEP, STARVE_WAKES, STEP_JITTER, STEP_TICKS};

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
    for seed in 0..6000 {
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
    assert!(eaten > 10_000 && births > 100, "{eaten} eaten, {births} born");
    assert!(lambs_seen && pregnant_seen);
    assert_eq!(lost, 0, "no sheep walks off the superchunks held");
    assert!(world.entities.iter().any(|sheep| sheep.attribute(LAMB).is_none() && sheep.attribute(HUNGER).is_some()), "grown sheep");
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
