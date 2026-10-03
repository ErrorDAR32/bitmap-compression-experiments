//! The tick: rules run superchunk by superchunk, writes queued for the
//! superchunks they land in and applied in a second phase -- the same on
//! any number of threads, across borders, reading the world as the tick
//! found it, and never past the superchunks next door.
//!
//! `cargo test`

use bitplane_manager::{BitmapArena, BucketKey, Shape, Write, WriteOp};
use simulation::entity_store::Entities;
use simulation::{Simulation, Turn, AREA_CENTRE, AREA_SIDE};
use chunk_storage::{LayerCodec, LayerType};
use coordinates::{CartesianCell, CellIndex, ChunkPlace, ChunkPosition, SuperchunkPosition, SUPERCHUNK_SIDE_CELLS};

/// The layer type the tests run on.
const STONE: LayerType = LayerType(6);

/// An arena with `STONE` hot over the `side` by `side` superchunks from
/// `(10, 10)`, the cells `cells` set.
fn arena(side: u32, cells: impl Iterator<Item = CartesianCell>) -> BitmapArena {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    for y in 10..10 + side {
        for x in 10..10 + side {
            for place in ChunkPlace::all() {
                arena.make_hot(BucketKey { layer_type: STONE, chunk: ChunkPosition::of(SuperchunkPosition { x, y }, place) }, None, &mut codec);
            }
        }
    }
    cells.for_each(|cell| arena.queue(STONE, Write::cell(cell.into(), WriteOp::Set)));
    assert_eq!(arena.apply().missed, 0);
    arena
}

/// The first cell of the superchunk `(x, y)`.
fn corner(x: u32, y: u32) -> CartesianCell {
    CartesianCell { x: x * SUPERCHUNK_SIDE_CELLS, y: y * SUPERCHUNK_SIDE_CELLS }
}

/// Stone creeping: each cell sampled at 5% sets a random neighbour --
/// within the 3x3 around it -- or clears itself: how many it sampled.
fn creep(turn: &mut Turn, samples: &mut Vec<CellIndex>) -> usize {
    let sampled = turn.sample(STONE, 0.05, samples);
    for &cell in samples.iter() {
        let (dx, dy) = (turn.random().below(3) as i32 - 1, turn.random().below(3) as i32 - 1);
        if (dx, dy) == (0, 0) {
            turn.queue(STONE, Write::cell(cell, WriteOp::Unset));
        } else if let Some(neighbour) = cell.offset(dx, dy) {
            turn.queue(STONE, Write::cell(neighbour, WriteOp::Set));
        }
    }
    sampled
}

/// Every hot bitmap's cells, in the arena's order.
fn every_cell(arena: &BitmapArena) -> Vec<(BucketKey, Vec<u64>)> {
    arena.keys().map(|key| (key, arena.bucket(key).expect("hot").cells().to_vec())).collect()
}

/// Cells scattered over the 3x3 superchunks from `(10, 10)`, some on
/// their borders.
fn scattered() -> impl Iterator<Item = CartesianCell> {
    let start = corner(10, 10);
    (0..3000u32).map(move |at| CartesianCell { x: start.x + (at * 7919) % 3072, y: start.y + (at * 104_729) % 3072 })
}

/// A tick comes out the same on one thread and on four: each
/// superchunk's random numbers are its own, and the second phase applies
/// in a fixed order.
#[test]
fn any_number_of_threads_ticks_the_same() {
    let (mut one, mut four) = (arena(3, scattered()), arena(3, scattered()));
    for seed in 0..20 {
        let (a, b) = (Simulation::new(1).tick(&mut one, &mut Entities::new(), seed, creep), Simulation::new(4).tick(&mut four, &mut Entities::new(), seed, creep));
        assert_eq!((a.rules, a.writes_applied), (b.rules, b.writes_applied), "tick {seed}");
    }
    assert_eq!(every_cell(&one), every_cell(&four));
}

/// A write lands in the neighbour it falls in; reads in the first phase
/// see the world as the tick found it, writes queued or not.
#[test]
fn writes_cross_borders_and_reads_see_the_tick_start() {
    let edge = CartesianCell { x: corner(11, 10).x - 1, y: corner(10, 10).y + 500 };
    let mut arena = arena(2, [edge].into_iter());
    let across: CellIndex = CartesianCell { x: edge.x + 1, y: edge.y }.into();
    let report = Simulation::new(1).tick(&mut arena, &mut Entities::new(), 0, |turn, samples| {
        turn.sample(STONE, 1.0, samples);
        for &cell in samples.iter() {
            let right = cell.offset(1, 0).expect("in the world");
            turn.queue(STONE, Write::cell(right, WriteOp::Set));
            assert_eq!(turn.holds(STONE, right), Ok(false), "still as the tick found it");
        }
        samples.len()
    });
    assert_eq!((report.rules, report.writes_applied.changed), (1, 1));
    assert_eq!(arena.holds(STONE, across), Ok(true), "set in the neighbour");
    assert_eq!(arena.superchunk_count(STONE, SuperchunkPosition { x: 11, y: 10 }), 1);
}

/// A rectangle straddling the corner four superchunks meet at lands in
/// all four, each applying its own part.
#[test]
fn shapes_split_over_the_superchunks_they_cover() {
    let meet = corner(11, 11);
    let mut arena = arena(2, [CartesianCell { x: meet.x - 1, y: meet.y - 1 }].into_iter());
    let report = Simulation::new(2).tick(&mut arena, &mut Entities::new(), 0, |turn, samples| {
        turn.sample(STONE, 1.0, samples);
        for &cell in samples.iter() {
            turn.queue(STONE, Write { at: cell.offset(-1, -1).expect("in the world"), op: WriteOp::Set, shape: Shape::Rect { width: 4, height: 4 } });
        }
        0
    });
    assert_eq!(report.writes_applied.changed, 15, "the 4x4 from two up and left of the meeting point, one cell set already");
    for (x, y, cells) in [(10, 10, 4), (11, 10, 4), (10, 11, 4), (11, 11, 4)] {
        assert_eq!(arena.superchunk_count(STONE, SuperchunkPosition { x, y }), cells, "superchunk ({x}, {y})");
    }
}

/// Writes landing in a superchunk with no bitmap in use are missed, and
/// counted.
#[test]
fn writes_to_cold_neighbours_are_missed() {
    let edge = CartesianCell { x: corner(11, 10).x - 1, y: corner(10, 10).y + 3 };
    let mut arena = arena(1, [edge].into_iter());
    let report = Simulation::new(1).tick(&mut arena, &mut Entities::new(), 0, |turn, samples| {
        turn.sample(STONE, 1.0, samples);
        for &cell in samples.iter() {
            turn.queue(STONE, Write::cell(cell.offset(1, 0).expect("in the world"), WriteOp::Set));
        }
        0
    });
    assert_eq!((report.writes_applied.changed, report.writes_applied.missed), (0, 1));
}

/// A write two superchunks away is past the speed of light.
#[test]
#[should_panic(expected = "past the speed of light")]
fn writes_past_the_speed_of_light_panic() {
    let mut arena = arena(1, [corner(10, 10)].into_iter());
    Simulation::new(1).tick(&mut arena, &mut Entities::new(), 0, |turn, samples| {
        turn.sample(STONE, 1.0, samples);
        for &cell in samples.iter() {
            turn.queue(STONE, Write::cell(cell.offset(2 * SUPERCHUNK_SIDE_CELLS as i32, 0).expect("in the world"), WriteOp::Set));
        }
        0
    });
}

/// The area about a cell read at once is its cells read one by one:
/// inside a superchunk, across the borders of four, and at the edge of
/// the hot superchunks, where some cells are not hot.
#[test]
fn areas_read_at_once_are_the_cells_read_one_by_one() {
    let mut arena = arena(3, scattered());
    let start = corner(10, 10);
    let centres = [(300, 300), (1024, 1024), (1020, 1029), (5, 5), (2040, 2047), (1024, 3), (777, 1023)];
    let checked = Simulation::new(1).tick(&mut arena, &mut Entities::new(), 0, |turn, _| {
        if turn.superchunk() != (SuperchunkPosition { x: 10, y: 10 }) {
            return 0;
        }
        for (x, y) in centres {
            let centre = CartesianCell { x: start.x + x, y: start.y + y };
            let area = turn.area(STONE, centre.into());
            for (across, down) in (0..AREA_SIDE as u32).flat_map(|down| (0..AREA_SIDE as u32).map(move |across| (across, down))) {
                let cell = CartesianCell { x: centre.x + across - AREA_CENTRE as u32, y: centre.y + down - AREA_CENTRE as u32 };
                let held = turn.holds(STONE, cell.into());
                let read = |rows: [u16; AREA_SIDE]| rows[down as usize] >> across & 1 == 1;
                assert_eq!((read(area.hot), read(area.set)), (held.is_ok(), held == Ok(true)), "({across}, {down}) of the area about ({x}, {y})");
            }
        }
        centres.len()
    });
    assert_eq!(checked.rules, centres.len());
}
