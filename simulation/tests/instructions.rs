//! What a rule does to entities, an instruction each: a step and a
//! sleep carry no attributes and keep them, across chunks and
//! superchunks; a step onto a taken cell is turned back; one entity
//! edits another, an attribute at a time; an entity changed is put
//! whole only if an attribute was; and the cells beside it, the free
//! ones among them and the way to a cell are asked of the turn.
//!
//! `cargo test`

use bitplane_manager::{BitmapArena, BucketKey};
use chunk_storage::{LayerCodec, LayerType};
use coordinates::{CartesianCell, CellIndex, ChunkPlace, ChunkPosition, SuperChunkPosition, SUPERCHUNK_SIDE_CELLS};
use simulation::around::{self, CENTRE, RING};
use simulation::entities::{Attribute, AttributeType, Edit, Entities, EntityId, EntityType, Header, NEVER};
use simulation::{Simulation, SuperChunkTick};
use std::sync::Mutex;

/// The layer type the arena holds: every cell hot, none set.
const STONE: LayerType = LayerType(6);
/// The entities' type.
const WALKER: EntityType = EntityType(40);
/// An attribute.
const NAME: AttributeType = AttributeType(41);
/// Another.
const MARK: AttributeType = AttributeType(42);
/// A third.
const SCAR: AttributeType = AttributeType(43);

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

/// Every walker woken steps a cell to the right, to wake next tick.
fn step_right(turn: &mut SuperChunkTick, _: &mut Vec<CellIndex>) -> usize {
    let now = turn.now();
    for entity in turn.woken() {
        turn.step(&entity.header, entity.header.at.offset(1, 0).expect("in the world"), now + 1);
    }
    0
}

/// Entities stepping keep their attributes though none is carried: in
/// their chunk, into the next, and into the next superchunk, where they
/// go whole.
#[test]
fn a_step_carries_no_attributes_and_keeps_them() {
    let (mut arena, mut entities) = world(2);
    let attributes = [Attribute { kind: NAME, value: 7 }, Attribute { kind: MARK, value: 9 }];
    // In a chunk, over a chunk's border, over a superchunk's.
    for (id, x) in [(1, 40), (2, 250), (3, 1020)] {
        entities.queue_put(walker(id, cell(x, 30), 0), &attributes);
    }
    entities.apply();
    let mut simulation = Simulation::new(2);
    let (mut moves, mut puts) = (0, 0);
    for seed in 0..10 {
        let report = simulation.tick(&mut arena, &mut entities, seed, step_right);
        (moves, puts) = (moves + report.entities.moves, puts + report.entities.puts);
    }
    for (id, x) in [(1, 50), (2, 260), (3, 1030)] {
        let entity = entities.get(EntityId(id), cell(x, 30)).expect("ten steps on");
        assert_eq!(entity.attributes, attributes);
    }
    assert_eq!(entities.len(), 3);
    // Every step a move but the one over the superchunk's border: put there as new, and where it stood until settled.
    assert_eq!((moves, puts), (29, 2));
}

/// A step onto a cell an entity stands on is turned back as it is
/// carried out: the stepper stays, and wakes when it was to all the
/// same.
#[test]
fn a_step_onto_a_taken_cell_is_turned_back() {
    let (mut arena, mut entities) = world(1);
    entities.queue_put(walker(1, cell(40, 30), 0), &[Attribute { kind: NAME, value: 1 }]);
    entities.queue_put(walker(2, cell(41, 30), NEVER), &[]);
    entities.apply();
    let mut simulation = Simulation::new(1);
    let (mut stayed, mut moves) = (0, 0);
    for seed in 0..3 {
        let report = simulation.tick(&mut arena, &mut entities, seed, step_right);
        (stayed, moves) = (stayed + report.entities.stayed, moves + report.entities.moves);
    }
    assert_eq!((stayed, moves), (3, 3), "turned back each tick, and woken the next");
    let stepper = entities.get(EntityId(1), cell(40, 30)).expect("where it stood");
    assert_eq!((stepper.header.wake, stepper.attribute(NAME)), (3, Some(1)));
    assert!(entities.get(EntityId(2), cell(41, 30)).is_some());
}

/// Entities edit another, an attribute each, in one tick: both land,
/// neither undoing the other, and the one edited keeps what it had and
/// when it wakes. An attribute is removed the same way.
#[test]
fn entities_edit_another_an_attribute_at_a_time() {
    let (mut arena, mut entities) = world(2);
    // The one edited, in another superchunk than one of the two editing it.
    let target = walker(9, cell(1030, 30), NEVER);
    entities.queue_put(target, &[Attribute { kind: NAME, value: 7 }]);
    entities.queue_put(walker(1, cell(1020, 30), 0), &[]);
    entities.queue_put(walker(2, cell(1040, 30), 0), &[]);
    entities.apply();
    let mut simulation = Simulation::new(2);
    let report = simulation.tick(&mut arena, &mut entities, 0, |turn: &mut SuperChunkTick, _: &mut Vec<CellIndex>| {
        for entity in turn.woken() {
            let kind = if entity.header.id == EntityId(1) { MARK } else { SCAR };
            turn.set_attribute(&target, kind, entity.header.id.0);
            turn.sleep(&entity.header, NEVER);
        }
        0
    });
    assert_eq!((report.entities.edits, report.entities.puts), (2, 0));
    let edited = entities.get(target.id, target.at).expect("where it stood");
    assert_eq!(edited.attributes, [Attribute { kind: NAME, value: 7 }, Attribute { kind: MARK, value: 1 }, Attribute { kind: SCAR, value: 2 }]);
    assert_eq!(edited.header, target);

    entities.queue_put(walker(3, cell(1031, 31), 1), &[]);
    entities.apply();
    simulation.tick(&mut arena, &mut entities, 1, |turn: &mut SuperChunkTick, _: &mut Vec<CellIndex>| {
        for entity in turn.woken() {
            turn.unset_attribute(&target, MARK);
            turn.set_attribute(&target, NAME, 8);
            turn.sleep(&entity.header, NEVER);
        }
        0
    });
    let edited = entities.get(target.id, target.at).expect("where it stood");
    assert_eq!(edited.attributes, [Attribute { kind: NAME, value: 8 }, Attribute { kind: SCAR, value: 2 }]);
}

/// An entity its rule looks over and leaves as it was is moved, or put
/// to sleep, with nothing carried; one with an attribute changed is put
/// whole.
#[test]
fn an_entity_is_put_whole_only_if_an_attribute_changed() {
    let (mut arena, mut entities) = world(1);
    entities.queue_put(walker(1, cell(40, 30), 0), &[Attribute { kind: NAME, value: 7 }]);
    entities.queue_put(walker(2, cell(40, 40), 0), &[Attribute { kind: NAME, value: 7 }]);
    entities.apply();
    let mut simulation = Simulation::new(1);
    let report = simulation.tick(&mut arena, &mut entities, 0, |turn: &mut SuperChunkTick, _: &mut Vec<CellIndex>| {
        let mut room = Vec::new();
        for entity in turn.woken() {
            let mut edit = Edit::of(entity, &mut room);
            // Set to what it is: no change. The second walker's is changed, and another added and removed.
            edit.set(NAME, 7);
            assert_eq!(edit.unset(MARK), None);
            assert!(!edit.edited());
            if entity.header.id == EntityId(2) {
                edit.set(NAME, 8);
                edit.set(MARK, 1);
                assert_eq!((edit.get(NAME), edit.get(MARK), edit.unset(MARK)), (Some(8), Some(1), Some(1)));
            }
            let to = entity.header.at.offset(1, 1).expect("in the world");
            turn.commit(edit, to, NEVER);
        }
        0
    });
    assert_eq!((report.entities.moves, report.entities.puts), (1, 1));
    assert_eq!(entities.get(EntityId(1), cell(41, 31)).expect("moved").attributes, [Attribute { kind: NAME, value: 7 }]);
    assert_eq!(entities.get(EntityId(2), cell(41, 41)).expect("moved").attributes, [Attribute { kind: NAME, value: 8 }]);
}

/// The nine cells about an entity as bits: which entities stand on,
/// which are free among those open to it, and none when all are taken.
#[test]
fn the_cells_beside_an_entity_are_asked_as_masks() {
    let (mut arena, mut entities) = world(1);
    let at = cell(40, 30);
    entities.queue_put(walker(1, at, 0), &[]);
    // Up and left of it, and to its right.
    entities.queue_put(walker(2, cell(39, 29), NEVER), &[]);
    entities.queue_put(walker(3, cell(41, 30), NEVER), &[]);
    entities.apply();
    let seen = Mutex::new(Vec::new());
    let mut simulation = Simulation::new(1);
    simulation.tick(&mut arena, &mut entities, 0, |turn: &mut SuperChunkTick, _: &mut Vec<CellIndex>| {
        for entity in turn.woken() {
            let at = entity.header.at;
            let (stone, taken) = (turn.around(STONE, at), turn.around_occupied(at));
            let free = (0..64).filter_map(|_| turn.free_beside(at, RING)).fold(0u16, |free, bit| free | 1 << bit);
            let only = turn.free_beside(at, 1 << 0 | 1 << 5);
            let cells: Vec<CellIndex> = [0, 5].iter().filter_map(|&bit| around::cell(at, bit)).collect();
            seen.lock().unwrap().push((stone.set, stone.hot, taken, free, only, cells, around::bit_of(at, at.offset(-1, 1).unwrap())));
            turn.sleep(&entity.header, NEVER);
        }
        0
    });
    let taken = 1 << 0 | CENTRE | 1 << 5;
    assert_eq!(*seen.lock().unwrap(), [(0, around::ALL, taken, RING & !taken, None, vec![cell(39, 29), cell(41, 30)], 6)]);
}

/// The way to a cell is found a step at a time, round what cannot be
/// walked on and round the entities in the way; a cell out of the area
/// has none.
#[test]
fn a_step_to_a_cell_goes_round_what_is_in_the_way() {
    let (mut arena, mut entities) = world(1);
    let (from, to) = (cell(40, 30), cell(44, 30));
    entities.queue_put(walker(1, from, 0), &[]);
    // A wall of entities across the straight way, open only below.
    for y in 23..=30 {
        entities.queue_put(walker(10 + y as u64, cell(42, y), NEVER), &[]);
    }
    entities.apply();
    let mut simulation = Simulation::new(1);
    let mut steps = 0;
    while entities.get(EntityId(1), to).is_none() {
        simulation.tick(&mut arena, &mut entities, steps, |turn: &mut SuperChunkTick, _: &mut Vec<CellIndex>| {
            let now = turn.now();
            for entity in turn.woken() {
                let at = entity.header.at;
                let passable = turn.area(STONE, at).hot;
                assert_eq!(turn.step_to(at, at.offset(9, 0).unwrap(), &passable), None, "out of the area");
                let next = turn.step_to(at, to, &passable).expect("a way round");
                turn.step(&entity.header, next, now + 1);
            }
            0
        });
        steps += 1;
        assert!(steps <= 8, "no way found in the steps it takes");
    }
    // Down past the wall's end and up again: two cells across, one down, one up, the diagonals counted once.
    assert_eq!(steps, 4);
}
