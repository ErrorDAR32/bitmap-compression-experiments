//! A world saved and loaded: it goes on exactly as it would have -- to
//! the cell, the entity and the random number, however often it is
//! stopped -- its files are where
//! and what they are said to be, and files that are not a save are
//! refused.
//!
//! `cargo test`

use bitplane_manager::BitmapArena;
use chunk_storage::disk::{self, DiskError};
use chunk_storage::mock::{DIRT, GRASS};
use coordinates::CartesianCell;
use entity_rules::diagnostics::world::MockWorld;
use simulation::entity_store::{Attribute, Entities, Header};
use simulation::Simulation;
use std::path::PathBuf;
use world::{self, transient_data};

/// A folder of its own for the test `name`, emptied.
fn folder(name: &str) -> PathBuf {
    let folder = transient_data::saves().join("tests").join(name);
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

/// Every cell, every entity with its attributes, and the tick.
type Everything = (Vec<u64>, Vec<(Header, Vec<Attribute>)>, u64);

/// Every cell of grass and dirt, and every entity: what two worlds the
/// same hold the same.
fn everything(arena: &BitmapArena, entities: &Entities) -> Everything {
    let cells = [DIRT, GRASS].into_iter().flat_map(|layer| arena.run(layer)).flat_map(|(_, bucket)| bucket.cells().to_vec()).collect();
    let all = entities.iter().map(|entity| (entity.header, entity.attributes.to_vec())).collect();
    (cells, all, entities.now())
}

/// Grass and sheep ticked, saved, and ticked on; the save loaded and
/// ticked as far: the two are the same world, crossings and all.
#[test]
fn a_world_loaded_goes_on_as_the_one_saved() {
    let folder = folder("goes_on");
    let mut first = MockWorld::with_sheep(4, 300_000, 3_000);
    let mut simulation = Simulation::new(3);
    for _ in 0..1_500 {
        world::tick(&mut simulation, &mut first.arena, &mut first.entities, 7);
    }
    let saved = world::save(&folder, "Pasture", 7, &mut first.arena, &mut first.storage, &first.entities, &simulation).expect("saved");
    assert_eq!((saved.superchunks, saved.entities), (4, first.entities.len()));

    let mut second = world::load(&folder).expect("loaded");
    assert_eq!((second.info.name.as_str(), second.info.seed, second.info.tick), ("Pasture", 7, 1_500));
    assert_eq!(second.info.layers, [DIRT, GRASS]);
    assert!(everything(&first.arena, &first.entities) == everything(&second.arena, &second.entities), "loaded as saved");
    assert_eq!(simulation.random_states().collect::<Vec<_>>(), second.simulation.random_states().collect::<Vec<_>>());

    let (mut eaten, mut born) = (0, 0);
    for _ in 0..3_000 {
        let report = world::tick(&mut simulation, &mut first.arena, &mut first.entities, 7);
        world::tick(&mut second.simulation, &mut second.arena, &mut second.entities, 7);
        (eaten, born) = (eaten + report.rules.sheep.eaten, born + report.rules.sheep.births);
    }
    assert!(eaten > 1_000 && born > 10, "{eaten} eaten, {born} born: a world doing something");
    assert!(everything(&first.arena, &first.entities) == everything(&second.arena, &second.entities), "the same 3,000 ticks on");
    assert_eq!(simulation.random_states().collect::<Vec<_>>(), second.simulation.random_states().collect::<Vec<_>>());
}

/// A world saved and loaded again and again mid run is, at a tick
/// agreed, the world that ran straight to it: every cell, every entity,
/// every random number.
#[test]
fn a_world_saved_and_loaded_mid_run_comes_to_the_same() {
    const UNTIL: u64 = 4_000;
    let mut straight = world::generate(11, 4);
    while straight.entities.now() < UNTIL {
        world::tick(&mut straight.simulation, &mut straight.arena, &mut straight.entities, 11);
    }

    let folder = folder("mid_run");
    let mut stopped = world::generate(11, 4);
    for stop in [1, 700, 701, 1_900, 3_333, UNTIL] {
        while stopped.entities.now() < stop {
            world::tick(&mut stopped.simulation, &mut stopped.arena, &mut stopped.entities, 11);
        }
        world::save(&folder, "Stopped", 11, &mut stopped.arena, &mut stopped.storage, &stopped.entities, &stopped.simulation).expect("saved");
        // What ran is dropped whole: the next stretch runs on what the files hold alone.
        stopped = world::load(&folder).expect("loaded");
        assert_eq!(stopped.info.tick, stop);
    }
    assert!(straight.entities.len() > 16_000, "{} sheep: a flock that bred", straight.entities.len());
    assert!(everything(&straight.arena, &straight.entities) == everything(&stopped.arena, &stopped.entities), "the same at tick {UNTIL}");
    assert_eq!(straight.simulation.random_states().collect::<Vec<_>>(), stopped.simulation.random_states().collect::<Vec<_>>());
}

/// A save is a folder: a world file in text, and two files a
/// superchunk named by its Morton index in hexadecimal.
#[test]
fn a_save_is_a_directory_of_files_named_by_morton_index() {
    let folder = folder("files");
    let mut first = MockWorld::with_sheep(4, 1_000, 10);
    let simulation = Simulation::new(1);
    world::save(&folder, "Four fields", 99, &mut first.arena, &mut first.storage, &first.entities, &simulation).expect("saved");
    let text = std::fs::read_to_string(folder.join("world")).expect("the world's file");
    assert_eq!(text, "tilesim world 1\nname = Four fields\nseed = 99\ntick = 0\nlayers = 1 2\n");
    let mut names: Vec<String> = std::fs::read_dir(folder.join("superchunks")).expect("the superchunks").map(|entry| entry.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    let expected: Vec<String> = disk::saved_superchunks(&folder).expect("listed").iter().flat_map(|superchunk| ["image", "state"].map(|kind| format!("{:011x}.{kind}", superchunk.morton_index()))).collect();
    assert_eq!(names, expected);
    assert_eq!(names.len(), 8);
    let held: Vec<u64> = first.arena.superchunks().iter().map(|superchunk| superchunk.morton_index()).collect();
    assert_eq!(disk::saved_superchunks(&folder).unwrap().iter().map(|superchunk| superchunk.morton_index()).collect::<Vec<_>>(), held);
}

/// What is not a save is refused, saying which file and why.
#[test]
fn files_that_are_not_a_save_are_refused() {
    let folder = folder("refused");
    assert!(matches!(world::load(&folder), Err(DiskError::Io(..))), "no such folder");
    let mut first = MockWorld::with_sheep(1, 1_000, 10);
    let simulation = Simulation::new(1);
    world::save(&folder, "One", 1, &mut first.arena, &mut first.storage, &first.entities, &simulation).expect("saved");
    let state = std::fs::read_dir(folder.join("superchunks")).unwrap().map(|entry| entry.unwrap().path()).find(|path| path.extension().unwrap() == "state").unwrap();
    let whole = std::fs::read(&state).unwrap();
    std::fs::write(&state, &whole[..whole.len() - 8]).unwrap();
    assert!(matches!(world::load(&folder), Err(DiskError::Invalid(path, what)) if path == state && what == "cut short"));
    std::fs::write(&state, &whole).unwrap();
    assert!(world::load(&folder).is_ok());
    std::fs::write(folder.join("world"), "something else\n").unwrap();
    assert!(matches!(world::load(&folder), Err(DiskError::Invalid(..))));
}

/// In a world generated, the ground has walls, and no sheep ever steps
/// through one: across or down only between cells at most a step apart
/// in height, diagonally only where both ways round are such steps.
#[test]
fn sheep_never_step_through_a_wall() {
    use std::collections::HashMap;
    let seed = 5;
    let mut made = world::generate(seed, 4);
    let high = |at: coordinates::CellIndex| {
        let cell = at.cartesian();
        terrain::height(seed, cell.x, cell.y)
    };
    let mut stood: HashMap<u64, coordinates::CellIndex> = made.entities.iter().map(|sheep| (sheep.header.id.0, sheep.header.at)).collect();
    let (mut moved, mut beside_walls) = (0, 0);
    for _ in 0..1_500 {
        world::tick(&mut made.simulation, &mut made.arena, &mut made.entities, seed);
        for sheep in made.entities.iter() {
            let at = sheep.header.at;
            if let Some(was) = stood.insert(sheep.header.id.0, at).filter(|&was| was != at) {
                let (from, to) = (was.cartesian(), at.cartesian());
                // The cells of each way round: the straight step's alone, or the diagonal's two corners.
                let corners = [CartesianCell { x: to.x, y: from.y }, CartesianCell { x: from.x, y: to.y }];
                let walled = corners.iter().any(|corner| terrain::wall(high(was), high((*corner).into())) || terrain::wall(high((*corner).into()), high(at)));
                assert!(!walled, "from height {} to {}: {from:?} to {to:?}", high(was), high(at));
                moved += 1;
            }
            beside_walls += (0..9).any(|way| at.offset(way % 3 - 1, way / 3 - 1).is_some_and(|beside| terrain::wall(high(at), high(beside)))) as usize;
        }
    }
    assert!(moved > 1_000 && beside_walls > 10_000, "{moved} steps, {beside_walls} sheep-ticks beside a wall");
}
