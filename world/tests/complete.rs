//! The complete tier: larger worlds run for longer -- minutes at most.
//! The tiers: `docs/testing_protocol.md`, at the repository's root.
//!
//! `cargo test --release --test complete -- --ignored`

use bitplane_manager::BitmapArena;
use chunk_storage::mock::{DIRT, GRASS};
use simulation::entity_store::{Attribute, Entities, Header};
use world::transient_data;

/// Every cell, every entity with its attributes, and the tick.
type Everything = (Vec<u64>, Vec<(Header, Vec<Attribute>)>, u64);

/// Every cell of grass and dirt, and every entity.
fn everything(arena: &BitmapArena, entities: &Entities) -> Everything {
    let cells = [DIRT, GRASS].into_iter().flat_map(|layer| arena.run(layer)).flat_map(|(_, bucket)| bucket.cells().to_vec()).collect();
    let all = entities.iter().map(|entity| (entity.header, entity.attributes.to_vec())).collect();
    (cells, all, entities.now())
}

/// Sixteen superchunks run 30,000 ticks straight, and the same world
/// saved and loaded every 5,000: the same at the end.
#[test]
#[ignore]
fn a_large_world_stopped_often_comes_to_the_same() {
    const UNTIL: u64 = 30_000;
    let mut straight = world::generate(21, 16);
    while straight.entities.now() < UNTIL {
        world::tick(&mut straight.simulation, &mut straight.arena, &mut straight.entities, 21);
    }
    let folder = transient_data::saves().join("tests").join("complete");
    let _ = std::fs::remove_dir_all(&folder);
    let mut stopped = world::generate(21, 16);
    for stop in (5_000..=UNTIL).step_by(5_000) {
        while stopped.entities.now() < stop {
            world::tick(&mut stopped.simulation, &mut stopped.arena, &mut stopped.entities, 21);
        }
        world::save(&folder, "Stopped", 21, &mut stopped.arena, &mut stopped.storage, &stopped.entities, &stopped.simulation).expect("saved");
        stopped = world::load(&folder).expect("loaded");
    }
    assert!(everything(&straight.arena, &straight.entities) == everything(&stopped.arena, &stopped.entities), "the same at tick {UNTIL}");
    assert_eq!(straight.simulation.random_states().collect::<Vec<_>>(), stopped.simulation.random_states().collect::<Vec<_>>());
}

/// A flock on generated ground, walls and all, neither dies out nor
/// overruns its pasture in 300,000 ticks.
#[test]
#[ignore]
fn a_flock_on_generated_ground_lasts() {
    let mut made = world::generate(3, 4);
    for _ in 0..300_000 {
        world::tick(&mut made.simulation, &mut made.arena, &mut made.entities, 3);
    }
    let grass: u64 = made.arena.superchunks().iter().map(|superchunk| made.arena.superchunk_count(GRASS, superchunk.position()) as u64).sum();
    let sheep = made.entities.len();
    assert!((500..200_000).contains(&sheep), "{sheep} sheep");
    assert!(grass > 4 * 1024 * 1024 / 20, "{grass} cells of grass");
}
