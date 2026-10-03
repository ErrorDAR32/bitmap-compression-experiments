//! Entities ticked: woken at their tick and no other, near or far off;
//! attributes added and removed at run time; moving across superchunk
//! borders as whole copies; never two on a cell; read across
//! superchunks as the tick found them; held in Morton order by cell
//! however they step; and the same on any number of threads.
//!
//! `cargo test`

use bitplane_manager::{BitmapArena, BucketKey};
use chunk_storage::{LayerCodec, LayerType};
use coordinates::{CartesianCell, CellIndex, ChunkPlace, ChunkPosition, SuperChunkPosition, SUPERCHUNK_SIDE_CELLS};
use simulation::entities::{remove_attribute, set_attribute, AttributeType, Entities, EntityId, EntityType, Header, NEVER, OCCUPIED, WHEEL_TICKS};
use simulation::{Simulation, SuperChunkTick};
use std::sync::Mutex;

/// The layer type the arena holds: none of its cells are read.
const STONE: LayerType = LayerType(6);
/// The entities' type.
const WALKER: EntityType = EntityType(40);
/// An attribute counting the times an entity woke.
const WOKEN: AttributeType = AttributeType(41);
/// An attribute present every other time an entity woke.
const ODD: AttributeType = AttributeType(42);

/// An arena with a bitmap hot over the `side` by `side` superchunks from
/// `(10, 10)`, and entities holding the same superchunks.
fn world(side: u32) -> (BitmapArena, Entities) {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    for y in 10..10 + side {
        for x in 10..10 + side {
            for place in ChunkPlace::all() {
                arena.make_hot(BucketKey { layer_type: STONE, chunk: ChunkPosition::of(SuperChunkPosition { x, y }, place) }, None, &mut codec);
            }
        }
    }
    let mut entities = Entities::new();
    let mortons: Vec<u64> = arena.superchunks().iter().map(|superchunk| superchunk.morton()).collect();
    assert_eq!(entities.align(&mortons), 0);
    (arena, entities)
}

/// The cell `(x, y)` cells from the top left of the superchunk `(10, 10)`.
fn cell(x: u32, y: u32) -> CellIndex {
    CartesianCell { x: 10 * SUPERCHUNK_SIDE_CELLS + x, y: 10 * SUPERCHUNK_SIDE_CELLS + y }.into()
}

/// A walker with ID `id` on `at`, waking at `wake`.
fn walker(id: u64, at: CellIndex, wake: u64) -> Header {
    Header { id: EntityId(id), kind: WALKER, at, wake }
}

/// Each walker woken counts it, flips `ODD`, and wakes next tick, where
/// it stands: how many woke.
fn count_and_flip(turn: &mut SuperChunkTick, _: &mut Vec<CellIndex>) -> usize {
    let mut woken = 0;
    let mut attributes = Vec::new();
    for entity in turn.woken() {
        attributes.clear();
        attributes.extend_from_slice(entity.attributes);
        set_attribute(&mut attributes, WOKEN, entity.attribute(WOKEN).unwrap_or(0) + 1);
        if remove_attribute(&mut attributes, ODD).is_none() {
            set_attribute(&mut attributes, ODD, 1);
        }
        turn.put(Header { wake: turn.now() + 1, ..entity.header }, &attributes);
        woken += 1;
    }
    woken
}

/// Attributes added and removed every tick, by hundreds of entities in
/// one chunk -- so the old runs pile up as garbage and are swept -- come
/// out right.
#[test]
fn attributes_come_and_go_at_run_time() {
    let (mut arena, mut entities) = world(1);
    for id in 0..300 {
        entities.queue_put(walker(id * 7919 + 1, cell(id as u32 % 200, 5 + id as u32 / 200), 0), &[]);
    }
    assert_eq!(entities.queued(), 300);
    assert_eq!(entities.apply(&mut arena).puts, 300);
    assert_eq!((entities.len(), entities.queued()), (300, 0));
    let mut simulation = Simulation::new(1);
    for tick in 1..=101u64 {
        let report = simulation.tick(&mut arena, &mut entities, tick, count_and_flip);
        assert_eq!((report.rules, report.entities.puts), (300, 300));
    }
    assert_eq!(entities.len(), 300);
    for entity in entities.iter() {
        assert_eq!(entity.attribute(WOKEN), Some(101));
        assert_eq!(entity.attribute(ODD), Some(1), "flipped 101 times");
        assert_eq!(entity.attributes.len(), 2);
    }
}

/// An entity wakes at its tick and no other: one in reach of the wheel,
/// one past it, and none once removed.
#[test]
fn entities_wake_at_their_tick() {
    let (mut arena, mut entities) = world(1);
    let far = 2 * WHEEL_TICKS + 300;
    entities.queue_put(walker(1, cell(3, 3), 5), &[]);
    entities.queue_put(walker(2, cell(900, 900), far), &[]);
    entities.queue_put(walker(3, cell(10, 10), 7), &[]);
    entities.apply(&mut arena);
    let woken = Mutex::new(Vec::new());
    let mut simulation = Simulation::new(1);
    for tick in 0..=far + 10 {
        simulation.tick(&mut arena, &mut entities, tick, |turn, _| {
            for entity in turn.woken() {
                woken.lock().unwrap().push((entity.header.id.0, turn.now()));
                if entity.header.id == EntityId(3) {
                    turn.remove(&entity.header);
                } else if entity.header.id == EntityId(1) && turn.now() == 5 {
                    turn.put(Header { wake: 40, ..entity.header }, entity.attributes);
                }
            }
            0
        });
    }
    assert_eq!(woken.into_inner().unwrap(), [(1, 5), (3, 7), (1, 40), (2, far)]);
    assert_eq!(entities.len(), 2, "the third removed");
}

/// Walkers stepping right a cell a tick cross into the next superchunk,
/// as whole copies, attributes and all -- the one left behind gone a
/// tick later -- and at the edge of the superchunks held stay, lost to
/// nowhere.
#[test]
fn entities_cross_borders_and_stay_at_the_edge_of_the_world_held() {
    let (mut arena, mut entities) = world(2);
    let start = SUPERCHUNK_SIDE_CELLS - 3;
    entities.queue_put(walker(9, cell(start, 100), 0), &[simulation::entities::Attribute { kind: WOKEN, value: 0 }]);
    entities.apply(&mut arena);
    let mut simulation = Simulation::new(2);
    let step = |turn: &mut SuperChunkTick, _: &mut Vec<CellIndex>| {
        for entity in turn.woken() {
            let after = Header { at: entity.header.at.offset(1, 0).unwrap(), wake: turn.now() + 1, ..entity.header };
            turn.update(&entity.header, after, &[simulation::entities::Attribute { kind: WOKEN, value: entity.attribute(WOKEN).unwrap() + 1 }]);
        }
        0
    };
    let mut most = 0;
    for tick in 0..10 {
        simulation.tick(&mut arena, &mut entities, tick, step);
        most = most.max(entities.len());
    }
    assert_eq!(most, 2, "here and there for the tick it crossed in");
    let right = SuperChunkPosition { x: 11, y: 10 }.morton_index();
    let entity = entities.superchunk(right).and_then(|superchunk| superchunk.iter().next()).expect("in the right neighbour");
    assert_eq!((entity.header.at, entity.attribute(WOKEN)), (cell(start + 10, 100), Some(10)));
    assert_eq!(entities.len(), 1, "the one left behind removed");
    for tick in 10..1100 {
        simulation.tick(&mut arena, &mut entities, tick, step);
    }
    let all: Vec<_> = entities.iter().map(|entity| entity.header.at).collect();
    assert_eq!(all, [cell(2 * SUPERCHUNK_SIDE_CELLS - 1, 100)], "at the edge of the superchunks held, not lost past it");
}

/// Walkers wandering at random, giving birth and dying over 3x3
/// superchunks, across their borders: the same on one thread and four.
#[test]
fn any_number_of_threads_ticks_entities_the_same() {
    let wander = |turn: &mut SuperChunkTick, _: &mut Vec<CellIndex>| {
        let mut changes = 0;
        for entity in turn.woken() {
            let header = entity.header;
            let (dx, dy) = (turn.random().below(3) as i32 - 1, turn.random().below(3) as i32 - 1);
            let at = header.at.offset(dx * 7, dy * 7).unwrap();
            let wake = turn.now() + 1 + turn.random().below(5);
            match turn.random().below(40) {
                0 => turn.remove(&header),
                1 => {
                    let child = Header { id: turn.new_id(), at, wake, ..header };
                    turn.put(child, &[]);
                    turn.put(Header { wake: turn.now() + 3, ..header }, entity.attributes);
                }
                _ => turn.update(&header, Header { at, wake, ..header }, entity.attributes),
            }
            changes += 1;
        }
        changes
    };
    let run = |threads| {
        let (mut arena, mut entities) = world(3);
        for id in 0..2000u64 {
            entities.queue_put(walker(id + 1, cell(((id * 7919) % 3072) as u32, ((id * 104_729) % 3072) as u32), id % 4), &[]);
        }
        entities.apply(&mut arena);
        let mut simulation = Simulation::new(threads);
        let changes: usize = (0..300).map(|tick| simulation.tick(&mut arena, &mut entities, tick, wander).rules).sum();
        let all: Vec<(Header, Vec<_>)> = entities.iter().map(|entity| (entity.header, entity.attributes.to_vec())).collect();
        (changes, all)
    };
    let (one, four) = (run(1), run(4));
    assert!(one.0 > 10_000 && !one.1.is_empty());
    assert_eq!(one, four);
}

/// Changes queued outside a tick land where their cells are, and one
/// past the superchunks held is lost; a removal takes its entity out.
#[test]
fn changes_outside_a_tick_are_queued_then_applied() {
    let (mut arena, mut entities) = world(1);
    let (here, away) = (walker(1, cell(5, 5), 0), walker(2, cell(2 * SUPERCHUNK_SIDE_CELLS, 5), 0));
    entities.queue_put(here, &[]);
    entities.queue_put(away, &[]);
    assert_eq!(entities.len(), 0, "nothing changes until applied");
    let applied = entities.apply(&mut arena);
    assert_eq!((applied.puts, applied.lost, entities.len()), (1, 1, 1));
    assert_eq!(entities.get(here.id, here.at).map(|entity| entity.header), Some(here));
    entities.queue_remove(&here);
    assert_eq!((entities.apply(&mut arena).removes, entities.len()), (1, 0));
}

/// A turn reads entities in its neighbours, by ID and by chunk, as the
/// tick found them -- changes queued this tick unseen.
#[test]
fn turns_read_entities_across_superchunks() {
    let (mut arena, mut entities) = world(2);
    let (left, right) = (walker(1, cell(SUPERCHUNK_SIDE_CELLS - 1, 7), 0), walker(2, cell(SUPERCHUNK_SIDE_CELLS, 7), NEVER));
    entities.queue_put(left, &[]);
    entities.queue_put(right, &[]);
    entities.apply(&mut arena);
    let seen = Mutex::new(Vec::new());
    Simulation::new(2).tick(&mut arena, &mut entities, 0, |turn, _| {
        for entity in turn.woken() {
            let neighbour = entity.header.at.offset(1, 0).unwrap();
            let by_id = turn.entity(EntityId(2), neighbour).map(|other| other.header);
            let in_chunk: Vec<_> = turn.entities_in(neighbour.chunk()).expect("held").map(|other| other.header.id).collect();
            turn.remove(&entity.header);
            seen.lock().unwrap().push((by_id, in_chunk, turn.entity(entity.header.id, entity.header.at).is_some()));
        }
        0
    });
    assert_eq!(seen.into_inner().unwrap(), [(Some(right), vec![EntityId(2)], true)]);
    assert_eq!(entities.len(), 1);
    assert!(Entities::new().superchunk(0).is_none());
}

/// A superchunk's entities wake in Morton order, by cell then ID,
/// however their wakes were filed: queued between ticks, or put in a
/// tick.
#[test]
fn entities_wake_in_morton_order() {
    let (mut arena, mut entities) = world(1);
    for id in 0..500u64 {
        entities.queue_put(walker(1000 - id, cell(((id * 7919) % 1000) as u32, ((id * 104_729) % 1000) as u32), 0), &[]);
    }
    entities.apply(&mut arena);
    let mut simulation = Simulation::new(1);
    for tick in 0..3 {
        let order = Mutex::new(Vec::new());
        simulation.tick(&mut arena, &mut entities, tick, |turn, _| {
            for entity in turn.woken() {
                order.lock().unwrap().push((entity.header.at, entity.header.id));
                let at = entity.header.at.offset(1, 1).unwrap();
                turn.update(&entity.header, Header { at, wake: turn.now() + 1, ..entity.header }, &[]);
            }
            0
        });
        let order = order.into_inner().unwrap();
        assert_eq!(order.len(), 500);
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "tick {tick}: woken out of Morton order");
    }
}

/// Entities stepping about their chunk and across its edges stay held
/// in Morton order by cell, each found where it stands and nowhere
/// else, attributes its own.
#[test]
fn entities_stay_in_morton_order_as_they_step() {
    let (mut arena, mut entities) = world(1);
    for id in 0..600u64 {
        let at = cell(200 + (id % 20) as u32 * 3, 240 + (id / 20) as u32 * 3);
        entities.queue_put(walker(id + 1, at, 0), &[simulation::entities::Attribute { kind: WOKEN, value: id }]);
    }
    entities.apply(&mut arena);
    let mut simulation = Simulation::new(1);
    for tick in 0..200 {
        simulation.tick(&mut arena, &mut entities, tick, |turn, _| {
            for entity in turn.woken() {
                let (dx, dy) = (turn.random().below(5) as i32 - 2, turn.random().below(5) as i32 - 2);
                let after = Header { at: entity.header.at.offset(dx, dy).unwrap(), wake: turn.now() + 1, ..entity.header };
                turn.update(&entity.header, after, entity.attributes);
            }
            0
        });
        let held: Vec<_> = entities.iter().map(|entity| (entity.header.at, entity.header.id)).collect();
        assert_eq!(held.len(), 600);
        assert!(held.windows(2).all(|pair| pair[0] < pair[1]), "tick {tick}: held out of Morton order");
    }
    for entity in entities.iter() {
        let header = entity.header;
        assert_eq!(entity.attribute(WOKEN), Some(header.id.0 - 1), "its own attributes");
        assert_eq!(entities.get(header.id, header.at).map(|found| found.header), Some(header));
        assert!(entities.get(header.id, header.at.offset(1, 0).unwrap()).is_none(), "found only where it stands");
    }
}

/// A change to an entity no longer where the tick found it is passed
/// over: of two moving one entity in a tick, the first wins, and it
/// wakes once.
#[test]
fn a_change_to_an_entity_that_moved_on_is_passed_over() {
    let (mut arena, mut entities) = world(1);
    entities.queue_put(walker(1, cell(50, 50), 0), &[]);
    entities.apply(&mut arena);
    let mut simulation = Simulation::new(1);
    let report = simulation.tick(&mut arena, &mut entities, 0, |turn, _| {
        for entity in turn.woken() {
            for step in [1, 2] {
                let after = Header { at: entity.header.at.offset(step, 0).unwrap(), wake: turn.now() + 1, ..entity.header };
                turn.update(&entity.header, after, &[]);
            }
        }
        0
    });
    assert_eq!((report.entities.puts, entities.len()), (1, 1));
    assert!(entities.get(EntityId(1), cell(51, 50)).is_some(), "the first move carried out");
    let woken = simulation.tick(&mut arena, &mut entities, 1, |turn, _| turn.woken().count()).rules;
    assert_eq!(woken, 1);
}

/// Entities never overlap: walkers crowded together, stepping at random
/// onto each other's cells, across chunk and superchunk borders, and
/// breeding onto cells drawn at random, stand one to a cell after every
/// tick -- and the bitplane of the cells they stand on says the same.
#[test]
fn entities_never_overlap() {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    for (x, y) in [(10, 10), (11, 10), (10, 11), (11, 11)] {
        for place in ChunkPlace::all() {
            arena.make_hot(BucketKey { layer_type: OCCUPIED, chunk: ChunkPosition::of(SuperChunkPosition { x, y }, place) }, None, &mut codec);
        }
    }
    let mut entities = Entities::new();
    // Crowded about the corner the four superchunks meet at, which is the corner of chunks too.
    let corner = SUPERCHUNK_SIDE_CELLS - 20;
    for id in 0..900u64 {
        entities.queue_put(walker(id + 1, cell(corner + (id % 30) as u32, corner + (id / 30) as u32), id % 3), &[]);
    }
    // Two on a cell already taken: refused.
    entities.queue_put(walker(5000, cell(corner, corner), 0), &[]);
    entities.queue_put(walker(5001, cell(corner + 7, corner + 7), 0), &[]);
    let applied = entities.apply(&mut arena);
    assert_eq!((applied.puts, applied.refused, entities.len()), (900, 2, 900));
    let jostle = |turn: &mut SuperChunkTick, _: &mut Vec<CellIndex>| {
        for entity in turn.woken() {
            let header = entity.header;
            let (dx, dy) = (turn.random().below(3) as i32 - 1, turn.random().below(3) as i32 - 1);
            let wake = turn.now() + 1 + turn.random().below(3);
            turn.update(&header, Header { at: header.at.offset(dx, dy).unwrap(), wake, ..header }, entity.attributes);
            if turn.random().below(50) == 0 {
                let born = header.at.offset(turn.random().below(5) as i32 - 2, turn.random().below(5) as i32 - 2).unwrap();
                let id = turn.new_id();
                turn.put(Header { id, at: born, wake, ..header }, &[]);
            }
        }
        0
    };
    let mut simulation = Simulation::new(4);
    let (mut stayed, mut refused, mut crossed) = (0, 0, 0);
    for tick in 0..400 {
        let report = simulation.tick(&mut arena, &mut entities, tick, jostle);
        (stayed, refused) = (stayed + report.entities.stayed, refused + report.entities.refused);
        crossed += entities.superchunks().iter().map(|superchunk| superchunk.crossings().len()).sum::<usize>();
        let mut cells: Vec<CellIndex> = entities.iter().map(|entity| entity.header.at).collect();
        cells.sort_unstable();
        assert!(cells.windows(2).all(|pair| pair[0] != pair[1]), "tick {tick}: two entities on a cell");
        let occupied: u32 = arena.superchunks().iter().map(|superchunk| superchunk.layer(OCCUPIED).expect("hot").hot_count()).sum();
        assert_eq!(occupied as usize, cells.len(), "tick {tick}: the cells occupied are the cells stood on");
        assert!(cells.iter().all(|&cell| arena.holds(OCCUPIED, cell) == Ok(true)), "tick {tick}: an entity on a cell not occupied");
    }
    assert!(stayed > 1000 && refused > 10 && crossed > 100, "{stayed} stayed, {refused} refused, {crossed} crossed");
    assert!(entities.len() > 900, "they bred");
}
